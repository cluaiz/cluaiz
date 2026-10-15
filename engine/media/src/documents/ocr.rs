//! Lazy, serialized Tesseract 5 C API. Both PDF pages and encoded images use RGB8.

use crate::native_assets::{load_library, symbol, NativeAssets};
use anyhow::{bail, Context, Result};
use cluaiz_core::{
    media::{DecodedImage, RGB8_CHANNELS},
    media_limits::MediaLimits,
};
use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    ptr::NonNull,
    sync::Mutex,
};

type Handle = *mut c_void;
type Create = unsafe extern "C" fn() -> Handle;
type Delete = unsafe extern "C" fn(Handle);

struct Functions {
    create: Create,
    delete: Delete,
    init: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    set_image: unsafe extern "C" fn(Handle, *const u8, c_int, c_int, c_int, c_int),
    recognize: unsafe extern "C" fn(Handle, Handle) -> c_int,
    text: unsafe extern "C" fn(Handle) -> *mut c_char,
    delete_text: unsafe extern "C" fn(*const c_char),
    confidence: unsafe extern "C" fn(Handle) -> c_int,
    clear: Delete,
    clear_adaptive: Delete,
    monitor_create: Create,
    monitor_delete: Delete,
    deadline: unsafe extern "C" fn(Handle, c_int),
}

struct Session {
    handle: NonNull<c_void>,
    functions: Functions,
    identity: String,
    deadline_ms: c_int,
    _library: libloading::Library,
}

// SAFETY: a session is accessed only while CACHE's exclusive mutex is held.
// It owns its native handle/library, with no external borrowed input or callbacks.
unsafe impl Send for Session {}

static CACHE: Mutex<Option<Session>> = Mutex::new(None);

impl Session {
    fn load(assets: &NativeAssets) -> Result<Self> {
        let config = assets
            .manifest
            .ocr
            .as_ref()
            .context("Native OCR is not included in this package")?;
        if config.deadline_ms == 0 {
            bail!("OCR deadline must be positive");
        }
        let deadline_ms = c_int::try_from(config.deadline_ms)?;
        let languages = config.languages.split('+').collect::<Vec<_>>();
        if languages.is_empty()
            || languages
                .iter()
                .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        {
            bail!("OCR languages must be explicit traineddata identifiers");
        }
        assets.verify()?;
        let data_path = assets.path(&config.data_directory)?;
        for language in languages {
            let data = data_path
                .join(format!("{language}.traineddata"))
                .canonicalize()?;
            if !assets
                .manifest
                .files
                .values()
                .any(|asset| assets.path(&asset.path).ok().as_ref() == Some(&data))
            {
                bail!("OCR language data is not listed in the verified package");
            }
        }
        let data_path = data_path.to_str().context("OCR data path must be UTF-8")?;
        // Tesseract joins this directory with '/' internally. Windows verbatim
        // paths disallow those separators; verification still used canonical paths.
        #[cfg(windows)]
        let data_path = if let Some(unc) = data_path.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            data_path
                .strip_prefix("\\\\?\\")
                .unwrap_or(data_path)
                .to_owned()
        };
        let data_path = CString::new(data_path)?;
        let language = CString::new(config.languages.as_str())?;
        let library = load_library(&assets.library(&config.library)?)?;
        // SAFETY: these signatures follow Tesseract's capi.h; the library stays owned.
        unsafe {
            let version: unsafe extern "C" fn() -> *const c_char =
                symbol(&library, b"TessVersion\0")?;
            let version = version();
            if version.is_null() || !CStr::from_ptr(version).to_bytes().starts_with(b"5.") {
                bail!("Native OCR requires the Tesseract 5 C ABI");
            }
            let functions = Functions {
                create: symbol(&library, b"TessBaseAPICreate\0")?,
                delete: symbol(&library, b"TessBaseAPIDelete\0")?,
                init: symbol(&library, b"TessBaseAPIInit3\0")?,
                set_image: symbol(&library, b"TessBaseAPISetImage\0")?,
                recognize: symbol(&library, b"TessBaseAPIRecognize\0")?,
                text: symbol(&library, b"TessBaseAPIGetUTF8Text\0")?,
                delete_text: symbol(&library, b"TessDeleteText\0")?,
                confidence: symbol(&library, b"TessBaseAPIMeanTextConf\0")?,
                clear: symbol(&library, b"TessBaseAPIClear\0")?,
                clear_adaptive: symbol(&library, b"TessBaseAPIClearAdaptiveClassifier\0")?,
                monitor_create: symbol(&library, b"TessMonitorCreate\0")?,
                monitor_delete: symbol(&library, b"TessMonitorDelete\0")?,
                deadline: symbol(&library, b"TessMonitorSetDeadlineMSecs\0")?,
            };
            let handle =
                NonNull::new((functions.create)()).context("Cannot create native OCR session")?;
            let session = Self {
                handle,
                functions,
                identity: assets.identity.clone(),
                deadline_ms,
                _library: library,
            };
            if (session.functions.init)(handle.as_ptr(), data_path.as_ptr(), language.as_ptr()) != 0
            {
                bail!("Cannot initialize native OCR with the packaged language data");
            }
            Ok(session)
        }
    }

    fn recognize(&mut self, image: &DecodedImage, limits: MediaLimits) -> Result<OcrText> {
        let width = c_int::try_from(image.width())?;
        let height = c_int::try_from(image.height())?;
        let stride = width
            .checked_mul(c_int::try_from(RGB8_CHANNELS)?)
            .context("OCR row stride overflow")?;
        limits.check_decoded_bytes(u64::try_from(image.pixels().len())?)?;
        // SAFETY: validated packed RGB buffer and dimensions remain alive until
        // recognition and clear finish; all native resources have scoped owners.
        let result = (|| unsafe {
            (self.functions.set_image)(
                self.handle.as_ptr(),
                image.pixels().as_ptr(),
                width,
                height,
                RGB8_CHANNELS as c_int,
                stride,
            );
            let monitor = NativeHandle::new(
                (self.functions.monitor_create)(),
                self.functions.monitor_delete,
            )?;
            (self.functions.deadline)(monitor.handle.as_ptr(), self.deadline_ms);
            self.read_result(monitor.handle.as_ptr(), limits, image.pixels().len())
        })();
        // SAFETY: same exclusively owned live handle. Clear on success AND failure;
        // preserve traineddata but no previous user's page or adaptive classification.
        unsafe {
            (self.functions.clear)(self.handle.as_ptr());
            (self.functions.clear_adaptive)(self.handle.as_ptr());
        }
        result
    }

    unsafe fn read_result(
        &self,
        monitor: Handle,
        limits: MediaLimits,
        image_bytes: usize,
    ) -> Result<OcrText> {
        if (self.functions.recognize)(self.handle.as_ptr(), monitor) != 0 {
            bail!("Native OCR recognition failed or reached its configured deadline");
        }
        let output = NativeText {
            pointer: NonNull::new((self.functions.text)(self.handle.as_ptr()))
                .context("Native OCR returned no text buffer")?,
            delete: self.functions.delete_text,
        };
        let bytes = CStr::from_ptr(output.pointer.as_ptr()).to_bytes();
        limits.check_decoded_bytes(u64::try_from(
            image_bytes
                .checked_add(
                    bytes
                        .len()
                        .checked_mul(2)
                        .context("OCR result copy size overflow")?,
                )
                .context("OCR text byte count overflow")?,
        )?)?;
        let text = std::str::from_utf8(bytes).context("Native OCR returned invalid UTF-8")?;
        if text.trim().is_empty() {
            bail!("Native OCR recognized no readable text");
        }
        let confidence = (self.functions.confidence)(self.handle.as_ptr());
        if !(0..=100).contains(&confidence) {
            bail!("Native OCR returned invalid confidence");
        }
        Ok(OcrText {
            text: text.to_owned(),
            confidence,
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner; no calls outlive it or its library.
        unsafe {
            (self.functions.delete)(self.handle.as_ptr());
        }
    }
}

struct NativeHandle {
    handle: NonNull<c_void>,
    delete: Delete,
}
impl NativeHandle {
    fn new(handle: Handle, delete: Delete) -> Result<Self> {
        Ok(Self {
            handle: NonNull::new(handle).context("Cannot allocate OCR monitor")?,
            delete,
        })
    }
}
impl Drop for NativeHandle {
    fn drop(&mut self) {
        // SAFETY: this owner holds the unique live monitor and its matching deleter.
        unsafe {
            (self.delete)(self.handle.as_ptr());
        }
    }
}
struct NativeText {
    pointer: NonNull<c_char>,
    delete: unsafe extern "C" fn(*const c_char),
}
impl Drop for NativeText {
    fn drop(&mut self) {
        // SAFETY: Tesseract allocated this unique NUL-terminated result; its
        // library and matching text deleter outlive the scoped result owner.
        unsafe {
            (self.delete)(self.pointer.as_ptr());
        }
    }
}

pub struct OcrText {
    pub text: String,
    pub confidence: i32,
}

/// Reuses one language session; package changes replace it under the same lock.
pub fn recognize(
    image: &DecodedImage,
    limits: MediaLimits,
    assets: &NativeAssets,
) -> Result<OcrText> {
    let mut cache = CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("Native OCR session lock is poisoned"))?;
    if cache
        .as_ref()
        .is_none_or(|session| session.identity != assets.identity)
    {
        // Drop the old session before loading another DLL or language package.
        *cache = None;
        *cache = Some(Session::load(assets)?);
    }
    cache
        .as_mut()
        .context("Native OCR session is unavailable")?
        .recognize(image, limits)
}

pub(super) fn unload() -> Result<()> {
    let mut cache = CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("Native OCR session lock is poisoned"))?;
    *cache = None;
    Ok(())
}
