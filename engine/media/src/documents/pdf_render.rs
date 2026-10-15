//! Serialized PDFium rendering. Retain one page bitmap, never a whole rasterized PDF.

use crate::native_assets::{load_library, symbol, NativeAssets};
use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedImage, media_limits::MediaLimits};
use std::{
    ffi::{c_char, c_int, c_ulong, c_void},
    ptr::NonNull,
    sync::Mutex,
};

type Handle = *mut c_void;
type Close = unsafe extern "system" fn(Handle);

struct Pdfium {
    load: unsafe extern "system" fn(*const c_void, usize, *const c_char) -> Handle,
    close_document: Close,
    count: unsafe extern "system" fn(Handle) -> c_int,
    load_page: unsafe extern "system" fn(Handle, c_int) -> Handle,
    close_page: Close,
    width: unsafe extern "system" fn(Handle) -> f32,
    height: unsafe extern "system" fn(Handle) -> f32,
    bitmap: unsafe extern "system" fn(c_int, c_int, c_int, *mut c_void, c_int) -> Handle,
    close_bitmap: Close,
    fill: unsafe extern "system" fn(Handle, c_int, c_int, c_int, c_int, c_ulong) -> c_int,
    render: unsafe extern "system" fn(Handle, Handle, c_int, c_int, c_int, c_int, c_int, c_int),
    destroy: unsafe extern "system" fn(),
    identity: String,
    dpi: u32,
    _library: libloading::Library,
}

static CACHE: Mutex<Option<Pdfium>> = Mutex::new(None);

impl Pdfium {
    fn load(assets: &NativeAssets) -> Result<Self> {
        let config = assets
            .manifest
            .pdf
            .as_ref()
            .context("PDF renderer is not included in this package")?;
        if config.dpi == 0 {
            bail!("PDF render DPI must be positive");
        }
        assets.verify()?;
        let library = load_library(&assets.library(&config.library)?)?;
        // SAFETY: signatures follow the stable PDFium fpdfview.h C ABI (stdcall
        // on Windows). Resolve ALL functions before initializing global state.
        unsafe {
            let init: unsafe extern "system" fn() = symbol(&library, b"FPDF_InitLibrary\0")?;
            let engine = Self {
                load: symbol(&library, b"FPDF_LoadMemDocument64\0")?,
                close_document: symbol(&library, b"FPDF_CloseDocument\0")?,
                count: symbol(&library, b"FPDF_GetPageCount\0")?,
                load_page: symbol(&library, b"FPDF_LoadPage\0")?,
                close_page: symbol(&library, b"FPDF_ClosePage\0")?,
                width: symbol(&library, b"FPDF_GetPageWidthF\0")?,
                height: symbol(&library, b"FPDF_GetPageHeightF\0")?,
                bitmap: symbol(&library, b"FPDFBitmap_CreateEx\0")?,
                close_bitmap: symbol(&library, b"FPDFBitmap_Destroy\0")?,
                fill: symbol(&library, b"FPDFBitmap_FillRect\0")?,
                render: symbol(&library, b"FPDF_RenderPageBitmap\0")?,
                destroy: symbol(&library, b"FPDF_DestroyLibrary\0")?,
                identity: assets.identity.clone(),
                dpi: config.dpi,
                _library: library,
            };
            init();
            Ok(engine)
        }
    }
}

impl Drop for Pdfium {
    fn drop(&mut self) {
        // SAFETY: global mutex excludes all users, all document/page/bitmap owners
        // were dropped before replacement, and the library is still owned here.
        unsafe {
            (self.destroy)();
        }
    }
}

struct NativeHandle {
    handle: NonNull<c_void>,
    close: Close,
}
impl NativeHandle {
    fn new(handle: Handle, close: Close, kind: &str) -> Result<Self> {
        Ok(Self {
            handle: NonNull::new(handle)
                .with_context(|| format!("Cannot open native PDF {kind}"))?,
            close,
        })
    }
}
impl Drop for NativeHandle {
    fn drop(&mut self) {
        // SAFETY: sole scoped handle owner; the serialized engine and matching
        // closer outlive the page/bitmap/document. No native call retains it.
        unsafe {
            (self.close)(self.handle.as_ptr());
        }
    }
}

pub struct PdfDocument<'a> {
    document: NativeHandle,
    engine: &'a Pdfium,
    _bytes: &'a [u8],
}

impl PdfDocument<'_> {
    pub fn page_count(&self) -> Result<u32> {
        // SAFETY: valid document under the global lock.
        let count = unsafe { (self.engine.count)(self.document.handle.as_ptr()) };
        if count <= 0 {
            bail!("PDF renderer found no pages");
        }
        Ok(u32::try_from(count)?)
    }

    /// One-based physical page number. Buffer and native bitmap die before return.
    pub fn render_page(&self, page: u32, limits: MediaLimits) -> Result<DecodedImage> {
        if page == 0 || page > self.page_count()? {
            bail!("PDF page number is out of range");
        }
        // SAFETY: the native document outlives this page, all calls are serialized.
        let page = unsafe {
            NativeHandle::new(
                (self.engine.load_page)(self.document.handle.as_ptr(), c_int::try_from(page - 1)?),
                self.engine.close_page,
                "page",
            )?
        };
        let (width, height) = unsafe {
            (
                (self.engine.width)(page.handle.as_ptr()),
                (self.engine.height)(page.handle.as_ptr()),
            )
        };
        let dimension = |points: f32| -> Result<c_int> {
            let pixels = (f64::from(points) * f64::from(self.engine.dpi) / 72.0).ceil();
            if !pixels.is_finite() || pixels < 1.0 || pixels > f64::from(c_int::MAX) {
                bail!("PDF page has invalid or excessive dimensions");
            }
            Ok(pixels as c_int)
        };
        let width = dimension(width)?;
        let height = dimension(height)?;
        let pixels = u64::try_from(width)?
            .checked_mul(u64::try_from(height)?)
            .context("PDF bitmap size overflow")?;
        // Native BGRA plus packed RGB coexist while converting. Native parser/font
        // workspaces are separate; this is not a total-process memory limit.
        limits.check_decoded_bytes(
            pixels
                .checked_mul(7)
                .context("PDF bitmap workspace overflow")?,
        )?;
        let stride = width.checked_mul(4).context("PDF bitmap stride overflow")?;
        let size = usize::try_from(pixels.checked_mul(4).context("PDF bitmap overflow")?)?;
        let mut bgra = Vec::new();
        bgra.try_reserve_exact(size)?;
        bgra.resize(size, 255);
        // SAFETY: owned contiguous BGRA allocation/stride stay valid until bitmap
        // close. PDFium format 4 is BGRA; flags 1 include visible annotations.
        unsafe {
            let bitmap = NativeHandle::new(
                (self.engine.bitmap)(width, height, 4, bgra.as_mut_ptr().cast(), stride),
                self.engine.close_bitmap,
                "bitmap",
            )?;
            if (self.engine.fill)(bitmap.handle.as_ptr(), 0, 0, width, height, 0xffff_ffff) == 0 {
                bail!("Cannot initialize the PDF page background");
            }
            (self.engine.render)(
                bitmap.handle.as_ptr(),
                page.handle.as_ptr(),
                0,
                0,
                width,
                height,
                0,
                1,
            );
        }
        let mut rgb = Vec::new();
        rgb.try_reserve_exact(usize::try_from(pixels * 3)?)?;
        for pixel in bgra.chunks_exact(4) {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        drop(bgra);
        Ok(DecodedImage::new(
            u32::try_from(width)?,
            u32::try_from(height)?,
            rgb.into(),
        )?)
    }
}

/// Scoped document ownership keeps source memory valid and serializes global
/// PDFium state across requests. Rendered canonical images may outlive the scope.
pub fn with_document<T>(
    bytes: &[u8],
    assets: &NativeAssets,
    action: impl FnOnce(&PdfDocument<'_>) -> Result<T>,
) -> Result<T> {
    let mut cache = CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF renderer lock is poisoned"))?;
    if cache
        .as_ref()
        .is_none_or(|engine| engine.identity != assets.identity)
    {
        *cache = None;
        *cache = Some(Pdfium::load(assets)?);
    }
    let engine = cache.as_ref().context("PDF renderer is unavailable")?;
    // SAFETY: borrowed source lives until document drop, before releasing cache.
    let document = unsafe {
        NativeHandle::new(
            (engine.load)(bytes.as_ptr().cast(), bytes.len(), std::ptr::null()),
            engine.close_document,
            "document",
        )?
    };
    action(&PdfDocument {
        document,
        engine,
        _bytes: bytes,
    })
}

pub(super) fn unload() -> Result<()> {
    let mut cache = CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF renderer lock is poisoned"))?;
    *cache = None;
    Ok(())
}
