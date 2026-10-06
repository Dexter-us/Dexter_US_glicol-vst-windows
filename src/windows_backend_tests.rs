//! Real native child creation without OpenGL; host DPI policy is not ours to set.
use baseview::{
    Event, EventStatus, Size, Window, WindowHandler, WindowOpenOptions, WindowScalePolicy,
};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    AreDpiAwarenessContextsEqual, CreateWindowExW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetThreadDpiAwarenessContext, GetWindowRect, MapWindowPoints, PeekMessageW, PM_REMOVE,
    WS_OVERLAPPEDWINDOW,
};

struct Parent(HWND);
impl Drop for Parent {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}
unsafe impl HasRawWindowHandle for Parent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Windows(raw_window_handle::windows::WindowsHandle {
            hwnd: self.0 as _,
            ..raw_window_handle::windows::WindowsHandle::empty()
        })
    }
}
struct Handler;
impl WindowHandler for Handler {
    fn on_frame(&mut self, _: &mut Window) {}
    fn on_event(&mut self, _: &mut Window, _: Event) -> EventStatus {
        EventStatus::Ignored
    }
}

#[test]
fn first_and_reopened_children_preserve_host_dpi_and_origin() {
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    unsafe {
        let parent = Parent(CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            100,
            120,
            320,
            240,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ));
        assert!(!parent.0.is_null());
        let before = GetThreadDpiAwarenessContext();
        for _ in 0..2 {
            let mut child = Window::open_parented(
                &parent,
                WindowOpenOptions {
                    title: "First-open geometry regression".into(),
                    size: Size::new(240.0, 160.0),
                    scale: WindowScalePolicy::ScaleFactor(1.0),
                },
                |_| Handler,
            );
            assert_ne!(
                AreDpiAwarenessContextsEqual(before, GetThreadDpiAwarenessContext()),
                0
            );
            let hwnd = match child.raw_window_handle() {
                RawWindowHandle::Windows(handle) => handle.hwnd as HWND,
                _ => panic!("Expected native Windows child"),
            };
            let mut client: RECT = std::mem::zeroed();
            assert_ne!(GetClientRect(hwnd, &mut client), 0);
            assert_eq!((client.right, client.bottom), (240, 160));
            let mut bounds: RECT = std::mem::zeroed();
            assert_ne!(GetWindowRect(hwnd, &mut bounds), 0);
            MapWindowPoints(
                std::ptr::null_mut(),
                parent.0,
                (&mut bounds as *mut RECT).cast(),
                2,
            );
            assert_eq!((bounds.left, bounds.top), (0, 0));
            child.close();
            let mut message = std::mem::zeroed();
            while PeekMessageW(&mut message, hwnd, 0, 0, PM_REMOVE) != 0 {
                DispatchMessageW(&message);
            }
            assert_ne!(
                AreDpiAwarenessContextsEqual(before, GetThreadDpiAwarenessContext()),
                0
            );
        }
    }
}
