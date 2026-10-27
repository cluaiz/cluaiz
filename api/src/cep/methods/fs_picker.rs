//! Foreground handling for the Engine's native picker (including browser callers).
//! Scope discovery to the picker thread, never a global window-title search.

#[cfg(not(windows))]
pub(super) fn open<T>(show: impl FnOnce() -> T) -> T {
    show()
}

#[cfg(windows)]
pub(super) use windows::open;

#[cfg(windows)]
mod windows {
    use std::{sync::mpsc, thread, time::Duration};

    const RETRY_INTERVAL: Duration = Duration::from_millis(50);
    const RETRY_LIMIT: usize = 60;
    const HWND_TOPMOST: isize = -1;
    const HWND_NOTOPMOST: isize = -2;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_SHOWWINDOW: u32 = 0x0040;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumThreadWindows(
            id: u32,
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetClassNameW(window: isize, name: *mut u16, capacity: i32) -> i32;
        fn IsWindowVisible(window: isize) -> i32;
        fn SetForegroundWindow(window: isize) -> i32;
        fn SetWindowPos(
            window: isize,
            after: isize,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }

    unsafe extern "system" fn raise_picker(window: isize, data: isize) -> i32 {
        let mut class = [0u16; 32];
        // SAFETY: EnumThreadWindows supplies a live HWND; class is writable for its full capacity.
        let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
        // #32770 is the Windows dialog class used by rfd's Common Item Dialog.
        if length <= 0 || String::from_utf16_lossy(&class[..length as usize]) != "#32770" {
            return 1;
        }
        // SAFETY: Only the enumerated picker thread's visible dialog can reach these calls.
        unsafe {
            if IsWindowVisible(window) == 0 {
                return 1;
            }
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW;
            // Restore the old front-raising behavior, without leaving the dialog always-on-top.
            SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, flags);
            SetForegroundWindow(window);
            SetWindowPos(window, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
            SetForegroundWindow(window);
            // data points to the local flag in the synchronous EnumThreadWindows call below.
            *(data as *mut bool) = true;
        }
        0
    }

    fn raise_once(thread_id: u32) -> bool {
        let mut found = false;
        // SAFETY: Enumeration is synchronous; the flag outlives every callback. The callback
        // only operates on windows created by the thread currently showing this one picker.
        unsafe { EnumThreadWindows(thread_id, raise_picker, &mut found as *mut bool as isize) };
        found
    }

    pub(crate) fn open<T>(show: impl FnOnce() -> T) -> T {
        // SAFETY: This returns the calling worker's ID; no handles are borrowed or owned.
        let picker_thread = unsafe { GetCurrentThreadId() };
        thread::scope(|scope| {
            let (done, stopped) = mpsc::channel::<()>();
            scope.spawn(move || {
                for _ in 0..RETRY_LIMIT {
                    match stopped.recv_timeout(RETRY_INTERVAL) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        // Closing/cancelling the picker drops the sender and ends polling.
                        _ => return,
                    }
                    if raise_once(picker_thread) {
                        return;
                    }
                }
            });
            let selected = show();
            drop(done);
            selected
        })
    }
}
