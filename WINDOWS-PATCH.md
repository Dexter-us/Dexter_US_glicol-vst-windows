# Windows focus and audio patch

This is a local patch copy of `https://github.com/glicol/glicol-vst`,
based on upstream main from 2023-03-15. It remains a **VST2 stereo effect**,
not VST3 and not a MIDI instrument. Nothing was changed in the upstream repository.
The plugin product name remains **Glicol VST**; the manufacturer shown by
VST hosts is **Dexter U.S.**

## Changes

- Clicking the Windows editor explicitly gives its child HWND keyboard focus.
  A chained Windows subclass also requests character, arrow and Tab keys from
  dialog-style hosts, and removes itself when the editor is destroyed.
- The existing egui/baseview versions are retained. There is no broad GUI-library
  migration. The Run button is always visible above a scrollable code editor.
- Host callbacks of any size are accumulated into the engine's 128-frame blocks.
  Every output sample is written, including callbacks smaller than 128 frames and
  non-multiple remainders. Stereo is preserved.
- This introduces a fixed **128-sample latency**, reported through VST
  `initial_delay` for host compensation. The first 128 samples after resume are
  silent by design. This is about 2.9 ms at 44.1 kHz or 2.7 ms at 48 kHz.
- The host sample rate is passed to a rebuilt engine while processing is suspended.
- Run submits an owned copy through a bounded queue, rather than sharing an unsafe
  pointer into the editable text. Closing or editing the window cannot invalidate
  the submitted string. A full queue produces a visible retry message.

## Build on Windows

1. Install 64-bit Rust (MSVC toolchain) and Visual Studio C++ Build Tools with the
   Windows SDK. Open a Developer PowerShell for VS.
2. Open this `glicol-vst` folder, then run:

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\scripts\windows-build.ps1
   ```

3. Close the DAW. Back up the existing plugin DLL and replace it with
   `target\release\glicol_vst.dll` in the **VST2** folder your host scans.
4. Remove duplicate copies from other scanned folders, then rescan the plugin.
   The DLL and DAW must both be 64-bit. Do not rename the file to `.vst3`.

`Cargo.lock` is included to make dependency resolution reproducible. The native
Windows build workflow can produce a DLL artifact when this folder is the root
of a GitHub fork. In the containing Replit project, build from `glicol-vst/`.

## Windows/Cakewalk acceptance checks

Windows keyboard behavior must still be checked in the actual DAW; a Linux build
or cross-compilation cannot prove that the host sends editor key messages.

1. Load as an audio-track **effect**. Click inside the text area and type, delete,
   use arrow keys, and paste with Ctrl+V. The cursor alone is not enough.
2. Confirm Run remains visible. Run a quiet tone, replacing the entire editor with:

   ```text
   o: sin 440 >> mul 0.1;
   ```

   Turn down monitoring first. Route the effect's stereo output to the master.
   This tests output independently of input routing; MIDI notes are not required.
3. Restore passthrough code and route a known audio clip/live audio into the effect:

   ```text
   o: ~input >> mul 0.1;
   ```

   The default is 20 dB quieter than the input, not unity gain.
4. Test host buffers 32, 64, 128, 192 (if available), 256 and 512 at 44.1/48 kHz.
   Output should continue after the initial one-block delay.
5. Close and reopen the editor several times. Type and Run again. Click back into
   the DAW and confirm it can regain keyboard focus.
6. If Cakewalk still intercepts typing, check its plugin-window keyboard-input
   control. Host-specific shortcut interception can override a child window.

## Boundaries

This patch does not add MIDI I/O, VST3, program persistence in saved DAW projects,
or parser-error reporting. The underlying engine may allocate when processing or
compiling code; this is not a claim of fully allocation-free realtime operation.

## Verification completed here

- Seven native audio/engine regression tests passed.
- Windows x64 GNU cross-compilation checks passed, including the Windows-only
  subclass regression test. That test is compiled here, not executed on Windows.
- Rust formatting and patch whitespace checks passed.
- Actual keyboard input and audio routing in Windows/Cakewalk remain unverified.
  The MSVC build script and Windows CI workflow run the native focus regression
  test, but a real DAW check is still required.
