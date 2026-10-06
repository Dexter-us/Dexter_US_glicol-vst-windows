//! Backport click-to-focus behavior without changing the pinned GUI backend.
//! The old baseview child window calls SetCapture, but not SetFocus.

use baseview::WindowHandle;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winapi::shared::basetsd::{DWORD_PTR, UINT_PTR};
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::commctrl::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use winapi::um::winuser::{
    SetFocus, DLGC_WANTALLKEYS, DLGC_WANTARROWS, DLGC_WANTCHARS, DLGC_WANTTAB, WM_GETDLGCODE,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_NCDESTROY, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
};

const SUBCLASS_ID: UINT_PTR = 0x474C4943;

pub fn install(handle: &WindowHandle) -> Result<(), String> {
    let hwnd = match handle.raw_window_handle() {
        RawWindowHandle::Windows(handle) => handle.hwnd as HWND,
        _ => return Err("Editor did not return a Windows window handle".to_owned()),
    };
    install_hwnd(hwnd)
}

fn install_hwnd(hwnd: HWND) -> Result<(), String> {
    // baseview creates its parented child synchronously on the calling UI thread.
    // SetWindowSubclass chains safely with the host and baseview window procedures.
    if unsafe { SetWindowSubclass(hwnd, Some(focus_proc), SUBCLASS_ID, 0) } == 0 {
        return Err(format!(
            "Could not install editor focus handler: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;
    use winapi::um::winuser::{
        CreateWindowExW, DestroyWindow, GetFocus, SendMessageW, WS_CHILD, WS_OVERLAPPEDWINDOW,
    };

    struct TestWindows(HWND);

    impl Drop for TestWindows {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn clicking_child_requests_focus_and_preserves_dialog_flags() {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "Focus test\0".encode_utf16().collect();
        unsafe {
            let parent = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                100,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            );
            assert!(!parent.is_null(), "Could not create test parent");
            let _cleanup = TestWindows(parent);
            let child = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_CHILD,
                0,
                0,
                100,
                100,
                parent,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            assert!(!child.is_null(), "Could not create test child");
            let before = SendMessageW(child, WM_GETDLGCODE, 0, 0);
            install_hwnd(child).unwrap();
            SetFocus(parent);
            SendMessageW(child, WM_LBUTTONDOWN, 0, 0);
            assert_eq!(GetFocus(), child);
            let flags = SendMessageW(child, WM_GETDLGCODE, 0, 0);
            let expected =
                (DLGC_WANTALLKEYS | DLGC_WANTARROWS | DLGC_WANTCHARS | DLGC_WANTTAB) as LRESULT;
            assert_eq!(flags & expected, expected);
            assert_eq!(flags & before, before);
            // The Drop guard destroys both windows and exercises subclass cleanup.
        }
    }
}

unsafe extern "system" fn focus_proc(
    hwnd: HWND,
    message: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: UINT_PTR,
    _reference_data: DWORD_PTR,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN | WM_MBUTTONDOWN | WM_RBUTTONDOWN | WM_XBUTTONDOWN => {
            // Focus only after a deliberate click, never every repaint. This
            // allows the host to regain focus when the user clicks elsewhere.
            SetFocus(hwnd);
        }
        WM_GETDLGCODE => {
            // Dialog-style hosts otherwise consume Tab/arrows/character keys.
            return DefSubclassProc(hwnd, message, wparam, lparam)
                | (DLGC_WANTALLKEYS | DLGC_WANTARROWS | DLGC_WANTCHARS | DLGC_WANTTAB) as LRESULT;
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(focus_proc), SUBCLASS_ID);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}
