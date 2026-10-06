# Glicol VST — separate Windows VST3 port

This is a **64-bit VST3 stereo audio effect**, not a MIDI instrument.
The product remains **Glicol VST**, manufactured by **Dexter U.S.**
The existing VST2 source on `main`, DLL, and workflow are unchanged. VST2 and VST3
have separate plugin identities and can be installed alongside each other.
The VST3 binary is compiled against a genuine VST3 factory and audio processor,
not produced by renaming the VST2 DLL.

## Build

Use Rust 1.88.0 or newer with the native `x86_64-pc-windows-msvc` toolchain
and Visual Studio C++ Build Tools/Windows SDK:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\windows-build.ps1
```

The result is a **directory bundle**:

```text
Glicol VST.vst3/
  Contents/
    x86_64-win/
      Glicol VST.vst3
```

The inner `.vst3` file is the compiled Windows DLL module.
Keep the directory structure intact when installing.
In the private GitHub repository this source lives at the root of the separate
`vst3` branch. The existing Windows workflow is reused unchanged on that branch.
Its artifact transports the raw VST3 module as `glicol_vst.dll` because that
filename is fixed in the workflow. The downloadable installation ZIP repackages
it inside the correct `.vst3` directory structure with this guide and licenses.
Do not install the raw CI DLL into a VST2 folder.

## Install alongside VST2

1. Close Cakewalk/your DAW completely.
2. Back up an existing **VST3** copy outside scanned plugin folders, if present.
   There is no need to remove or replace the working VST2 DLL.
3. Extract the ZIP and copy the **entire `Glicol VST.vst3` directory** to:

   ```text
   C:\Program Files\Common Files\VST3\
   ```

   Administrator permission may be required. Do not copy just the inner module.
4. Remove duplicate VST3 copies from other scanned folders, then rescan plugins.
5. In your 64-bit host, select the **VST3** edition of **Glicol VST** and insert
   it as an audio-track effect. It is not a MIDI instrument.

The adapter declares a fixed **128-sample latency** for host compensation.
The first 128 output samples after reset are silent by design: about 2.9 ms
at 44.1 kHz or 2.7 ms at 48 kHz.

Submitted programs are stored in the VST3 project/preset state. Click **Run**
before saving to commit an editor draft. Old VST2 instances and saved projects
are not automatically converted to VST3.

## Actual Windows/Cakewalk acceptance checklist

A compilation, factory test, or native Windows focus test does not prove that
Cakewalk delivers keyboard messages to the plugin editor. Host acceptance is
still required:

1. Insert the **VST3 audio effect**. Click in the text editor and type, delete,
   use arrow keys and Tab, and paste with Ctrl+V. Seeing a cursor is not enough.
2. Confirm **Run** remains visible above the scrollable code area.
3. Turn down monitoring, replace the entire code with a quiet test tone, and Run:

   ```text
   o: sin 440 >> mul 0.1;
   ```

   Route the stereo output to the master; MIDI notes are not required.
4. Run passthrough code and route a known clip/live audio into the effect:

   ```text
   o: ~input >> mul 0.1;
   ```

   This is 20 dB quieter than input, not unity gain.
5. Test host buffers 32, 64, 128, 192 if available, 256 and 512 at 44.1/48 kHz.
   Audio should continue after the initial 128-sample delay.
6. Close/reopen the editor several times, type and Run again, and click back
   into the DAW to confirm it can regain keyboard focus. Check Cakewalk's plugin
   window keyboard-input control if it intercepts shortcuts.
7. Run a changed program, save the project, close/reopen the project and confirm
   the submitted program and audio are restored.
8. Keep a VST2 instance on a different track and confirm both formats are usable.

## Scope and licensing

The original MIT license is retained. The VST3 wrapper uses the ISC-licensed
nice-plug framework and MIT/Apache-2.0 `vst3` bindings. Dependency notices are
provided with release packages.

No MIDI support or VST2-to-VST3 saved-project migration is added.
The Glicol engine may allocate while rendering/compiling code; this is not a
claim of allocation-free realtime processing. Parser-error reporting remains
limited to the underlying engine, as in the VST2 edition.
