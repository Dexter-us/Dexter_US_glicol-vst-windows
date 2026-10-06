# Embedded Windows DPI policy

Vendored unchanged from RustAudio/baseview revision
`f6e99e9aa6f5aeb6b721cb05e4d882a51d995909`, except this targeted patch.
Original MIT and Apache-2.0 licenses are retained.

Parented windows no longer call `SetProcessDpiAwarenessContext`. A plugin must
not change its host's process-wide display coordinate policy, especially after
creating the first HWND and renderer. The original backend changed it after
window creation, making the first opening different from subsequent openings.
Standalone windows retain the opt-in, moved before HWND creation.

The Cargo patch applies to both the plugin and egui-baseview's pinned Git
dependency so they use the same window types. Tests create real parented Win32
backend windows without OpenGL and check unchanged DPI context, child origin
and dimensions on first opening and reopening. Actual VSTHost/Cakewalk rendering
acceptance remains separate.
