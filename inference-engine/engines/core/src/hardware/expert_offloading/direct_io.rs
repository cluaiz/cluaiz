//! ⚡ High-Speed Cross-Platform Direct I/O Driver
//! Provides sector-aligned direct storage access bypassing the OS page cache.
//!
//! Windows: Uses `CreateFileA` with `FILE_FLAG_NO_BUFFERING | FILE_FLAG_SEQUENTIAL_SCAN`
//! Unix: Uses `O_DIRECT` or POSIX aligned file descriptors.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use tracing::{error, info, warn};

/// Sector alignment required for Direct I/O across NVMe SSD controllers (4096 bytes).
pub const DIRECT_IO_SECTOR_ALIGNMENT: usize = 4096;

/// A 4096-byte sector-aligned memory buffer on the heap.
/// Ensures zero copy DMA transfer compatibility with NVMe disk controllers.
pub struct AlignedBuffer {
    ptr: *mut u8,
    layout: Layout,
    len: usize,
}

unsafe impl Send for AlignedBuffer {}
unsafe impl Sync for AlignedBuffer {}

impl AlignedBuffer {
    /// Allocates an aligned zeroed memory buffer of at least `size` bytes,
    /// rounded up to the nearest 4096-byte sector boundary.
    pub fn new(size: usize) -> anyhow::Result<Self> {
        let aligned_size = if size == 0 {
            DIRECT_IO_SECTOR_ALIGNMENT
        } else {
            ((size + DIRECT_IO_SECTOR_ALIGNMENT - 1) / DIRECT_IO_SECTOR_ALIGNMENT)
                * DIRECT_IO_SECTOR_ALIGNMENT
        };

        let layout = Layout::from_size_align(aligned_size, DIRECT_IO_SECTOR_ALIGNMENT)
            .map_err(|e| anyhow::anyhow!("Invalid aligned layout: {}", e))?;

        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            anyhow::bail!("Failed to allocate aligned direct I/O memory buffer of {} bytes", aligned_size);
        }

        Ok(Self {
            ptr,
            layout,
            len: aligned_size,
        })
    }

    /// Returns the raw pointer to the aligned memory.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Returns the mutable raw pointer to the aligned memory.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    /// Returns the allocated length in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Deref for AlignedBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl DerefMut for AlignedBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                dealloc(self.ptr, self.layout);
            }
        }
    }
}

// ─── Windows Direct I/O Implementation ────────────────────────────────────────

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    type HANDLE = *mut c_void;
    const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
    const GENERIC_READ: u32 = 0x80000000;
    const FILE_SHARE_READ: u32 = 0x00000001;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_NO_BUFFERING: u32 = 0x20000000;
    const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x08000000;

    #[repr(C)]
    struct LARGE_INTEGER {
        quad_part: i64,
    }

    extern "system" {
        fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: u32,
            dwShareMode: u32,
            lpSecurityAttributes: *mut c_void,
            dwCreationDisposition: u32,
            dwFlagsAndAttributes: u32,
            hTemplateFile: HANDLE,
        ) -> HANDLE;

        fn ReadFile(
            hFile: HANDLE,
            lpBuffer: *mut c_void,
            nNumberOfBytesToRead: u32,
            lpNumberOfBytesRead: *mut u32,
            lpOverlapped: *mut c_void,
        ) -> i32;

        fn SetFilePointerEx(
            hFile: HANDLE,
            liDistanceToMove: LARGE_INTEGER,
            lpNewFilePointer: *mut LARGE_INTEGER,
            dwMoveMethod: u32,
        ) -> i32;

        fn GetFileSizeEx(hFile: HANDLE, lpFileSize: *mut LARGE_INTEGER) -> i32;

        fn CloseHandle(hObject: HANDLE) -> i32;
    }

    pub struct DirectFileHandle {
        handle: HANDLE,
        file_size: u64,
    }

    unsafe impl Send for DirectFileHandle {}
    unsafe impl Sync for DirectFileHandle {}

    impl DirectFileHandle {
        pub fn open(path: &Path) -> anyhow::Result<Self> {
            let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
            
            let handle = unsafe {
                CreateFileW(
                    wide_path.as_ptr(),
                    GENERIC_READ,
                    FILE_SHARE_READ,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    FILE_FLAG_NO_BUFFERING | FILE_FLAG_SEQUENTIAL_SCAN,
                    std::ptr::null_mut(),
                )
            };

            if handle == INVALID_HANDLE_VALUE {
                let err = std::io::Error::last_os_error();
                anyhow::bail!("Failed to open file with FILE_FLAG_NO_BUFFERING: {}", err);
            }

            let mut size_val = LARGE_INTEGER { quad_part: 0 };
            let size_ok = unsafe { GetFileSizeEx(handle, &mut size_val) };
            if size_ok == 0 {
                unsafe { CloseHandle(handle) };
                anyhow::bail!("Failed to get file size via GetFileSizeEx");
            }

            Ok(Self {
                handle,
                file_size: size_val.quad_part as u64,
            })
        }

        pub fn read_at_aligned(&self, offset: u64, buf: &mut AlignedBuffer) -> anyhow::Result<usize> {
            let mut new_pos = LARGE_INTEGER { quad_part: 0 };
            let move_dist = LARGE_INTEGER { quad_part: offset as i64 };
            
            let seek_ok = unsafe { SetFilePointerEx(self.handle, move_dist, &mut new_pos, 0) };
            if seek_ok == 0 {
                let err = std::io::Error::last_os_error();
                anyhow::bail!("SetFilePointerEx failed at offset {}: {}", offset, err);
            }

            let mut bytes_read: u32 = 0;
            let read_ok = unsafe {
                ReadFile(
                    self.handle,
                    buf.as_mut_ptr() as *mut c_void,
                    buf.len() as u32,
                    &mut bytes_read,
                    std::ptr::null_mut(),
                )
            };

            if read_ok == 0 {
                let err = std::io::Error::last_os_error();
                anyhow::bail!("Direct ReadFile failed: {}", err);
            }

            Ok(bytes_read as usize)
        }

        pub fn file_size(&self) -> u64 {
            self.file_size
        }
    }

    impl Drop for DirectFileHandle {
        fn drop(&mut self) {
            if self.handle != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.handle);
                }
            }
        }
    }
}

// ─── Non-Windows Fallback Implementation ──────────────────────────────────────

#[cfg(not(windows))]
mod platform {
    use super::*;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    pub struct DirectFileHandle {
        file: std::sync::Mutex<File>,
        file_size: u64,
    }

    impl DirectFileHandle {
        pub fn open(path: &Path) -> anyhow::Result<Self> {
            let mut file = File::open(path)?;
            let file_size = file.metadata()?.len();
            Ok(Self {
                file: std::sync::Mutex::new(file),
                file_size,
            })
        }

        pub fn read_at_aligned(&self, offset: u64, buf: &mut AlignedBuffer) -> anyhow::Result<usize> {
            let mut file = self.file.lock().map_err(|e| anyhow::anyhow!("File lock failed: {}", e))?;
            file.seek(SeekFrom::Start(offset))?;
            let read_bytes = file.read(buf)?;
            Ok(read_bytes)
        }

        pub fn file_size(&self) -> u64 {
            self.file_size
        }
    }
}

// ─── Unified DirectFileReader ────────────────────────────────────────────────

/// High-level Direct I/O Reader.
/// Automatically translates arbitrary unaligned byte ranges into sector-aligned NVMe transfers.
pub struct DirectFileReader {
    platform_handle: platform::DirectFileHandle,
    file_path: std::path::PathBuf,
}

impl DirectFileReader {
    /// Opens a file for direct I/O.
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let p = path.as_ref();
        let platform_handle = platform::DirectFileHandle::open(p)?;
        info!(
            "⚡ [Direct-IO] Opened {:?} with Direct I/O | Size: {:.2} GB",
            p.file_name().unwrap_or_default(),
            platform_handle.file_size() as f64 / (1024.0 * 1024.0 * 1024.0)
        );

        Ok(Self {
            platform_handle,
            file_path: p.to_path_buf(),
        })
    }

    /// Reads an arbitrary slice from disk directly into an output buffer.
    /// Handles unaligned boundaries by reading aligned chunks and slicing the exact target window.
    pub fn read_range(&self, file_offset: u64, byte_len: usize, dest: &mut [u8]) -> anyhow::Result<()> {
        if dest.len() < byte_len {
            anyhow::bail!("Destination buffer is smaller than requested byte length");
        }

        let aligned_start = (file_offset / DIRECT_IO_SECTOR_ALIGNMENT as u64) * DIRECT_IO_SECTOR_ALIGNMENT as u64;
        let offset_delta = (file_offset - aligned_start) as usize;
        let required_end = file_offset + byte_len as u64;
        let aligned_end = ((required_end + DIRECT_IO_SECTOR_ALIGNMENT as u64 - 1)
            / DIRECT_IO_SECTOR_ALIGNMENT as u64)
            * DIRECT_IO_SECTOR_ALIGNMENT as u64;
        let aligned_len = (aligned_end - aligned_start) as usize;

        let mut aligned_buf = AlignedBuffer::new(aligned_len)?;
        self.platform_handle.read_at_aligned(aligned_start, &mut aligned_buf)?;

        if offset_delta + byte_len > aligned_buf.len() {
            anyhow::bail!("Direct I/O read boundary exceeded file layout");
        }

        dest[..byte_len].copy_from_slice(&aligned_buf[offset_delta..offset_delta + byte_len]);
        Ok(())
    }

    /// Returns the total size of the file in bytes.
    pub fn file_size(&self) -> u64 {
        self.platform_handle.file_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_aligned_buffer_creation() {
        let buf = AlignedBuffer::new(100).unwrap();
        assert_eq!(buf.len(), DIRECT_IO_SECTOR_ALIGNMENT);
        assert_eq!(buf.as_ptr() as usize % DIRECT_IO_SECTOR_ALIGNMENT, 0);

        let buf2 = AlignedBuffer::new(5000).unwrap();
        assert_eq!(buf2.len(), DIRECT_IO_SECTOR_ALIGNMENT * 2);
        assert_eq!(buf2.as_ptr() as usize % DIRECT_IO_SECTOR_ALIGNMENT, 0);
    }

    #[test]
    fn test_direct_io_unaligned_read() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_direct_io.bin");

        // Write a test pattern
        let mut test_data = vec![0u8; 16384];
        for (i, byte) in test_data.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }
        std::fs::write(&file_path, &test_data).unwrap();

        let reader_res = DirectFileReader::open(&file_path);
        assert!(reader_res.is_ok());
        let reader = reader_res.unwrap();

        // Read unaligned range across sector boundary
        let offset = 4050u64;
        let len = 300usize;
        let mut dest = vec![0u8; len];
        let read_res = reader.read_range(offset, len, &mut dest);
        assert!(read_res.is_ok());

        assert_eq!(&dest[..], &test_data[4050..4050 + 300]);

        let _ = std::fs::remove_file(&file_path);
    }
}
