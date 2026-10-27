//! The production foreground helper, with opt-in real Windows Common Item Dialog checks.
#[path = "../../../api/src/cep/methods/fs_picker.rs"]
mod fs_picker;

#[test]
fn test_picker_cancel_and_reopen_preserve_result() {
    assert_eq!(fs_picker::open(|| None::<String>), None);
    assert_eq!(fs_picker::open(|| Some("selected")), Some("selected"));
}

#[test]
fn test_picker_panic_unwinds_and_can_reopen() {
    assert!(std::panic::catch_unwind(|| fs_picker::open(|| panic!("picker failed"))).is_err());
    assert_eq!(fs_picker::open(|| 7), 7);
}

#[cfg(windows)]
mod native {
    use super::fs_picker;
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

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
        fn GetForegroundWindow() -> isize;
        fn GetWindowLongW(window: isize, index: i32) -> i32;
        fn IsWindowVisible(window: isize) -> i32;
        fn PostMessageW(window: isize, message: u32, wparam: usize, lparam: isize) -> i32;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: isize,
            menu: isize,
            instance: isize,
            param: *mut std::ffi::c_void,
        ) -> isize;
        fn DestroyWindow(window: isize) -> i32;
    }

    unsafe extern "system" fn find_dialog(window: isize, data: isize) -> i32 {
        let mut class = [0u16; 32];
        // SAFETY: HWND comes from enumeration and the mutable buffer has the declared capacity.
        let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
        if length > 0 && String::from_utf16_lossy(&class[..length as usize]) == "#32770" {
            // SAFETY: The caller supplies a writable isize for this synchronous callback.
            unsafe { *(data as *mut isize) = window };
            return 0;
        }
        1
    }

    #[test]
    #[ignore = "opens and cancels two real Windows folder dialogs; run explicitly on a desktop"]
    fn test_native_picker_front_cancel_reopen_and_thread_isolation() {
        let root = tempfile::tempdir().unwrap();
        // SAFETY: This captures the test thread on which rfd will open each dialog.
        let picker_thread = unsafe { GetCurrentThreadId() };
        for _ in 0..2 {
            let (ready, started) = mpsc::channel();
            let observer = thread::spawn(move || {
                let class: Vec<u16> = "#32770\0".encode_utf16().collect();
                let title: Vec<u16> = "Open Folder\0".encode_utf16().collect();
                // SAFETY: System dialog class; owned test window, created and destroyed on this thread.
                let decoy = unsafe {
                    CreateWindowExW(
                        0,
                        class.as_ptr(),
                        title.as_ptr(),
                        0x10cf0000,
                        40,
                        40,
                        320,
                        160,
                        0,
                        0,
                        0,
                        std::ptr::null_mut(),
                    )
                };
                ready.send(decoy != 0).unwrap();
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut dialog = 0isize;
                let mut front = false;
                let mut ordinary = false;
                while Instant::now() < deadline {
                    // SAFETY: Only the test's picker thread is enumerated, with a live result pointer.
                    unsafe {
                        EnumThreadWindows(
                            picker_thread,
                            find_dialog,
                            &mut dialog as *mut isize as isize,
                        )
                    };
                    if dialog != 0 {
                        // SAFETY: Query the enumerated test dialog; GWL_EXSTYLE=-20, WS_EX_TOPMOST=8.
                        unsafe {
                            front = IsWindowVisible(dialog) != 0 && GetForegroundWindow() == dialog;
                            ordinary = GetWindowLongW(dialog, -20) & 8 == 0;
                        }
                        if front && ordinary {
                            break;
                        }
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                // SAFETY: Cancel only the test dialog; WM_CLOSE=0x10. Never send input to the decoy.
                let closed = dialog != 0 && unsafe { PostMessageW(dialog, 0x10, 0, 0) } != 0;
                // SAFETY: The decoy belongs to this thread and is not used after destruction.
                unsafe {
                    if decoy != 0 {
                        DestroyWindow(decoy);
                    }
                }
                (front, ordinary, closed, dialog != decoy)
            });
            assert!(started.recv_timeout(Duration::from_secs(2)).unwrap());
            let selected = fs_picker::open(|| {
                rfd::FileDialog::new()
                    .set_title("Open Folder")
                    .set_directory(root.path())
                    .pick_folder()
            });
            let observed = observer.join().unwrap();
            assert_eq!(selected, None, "native Cancel must remain null");
            assert_eq!(observed, (true, true, true, true), "picker must be foreground, non-topmost, cancellable and separate from same-title decoy");
        }
    }
}
