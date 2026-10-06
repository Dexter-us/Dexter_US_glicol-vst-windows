# Glicol VST — separate Windows VST3 port

This is a **64-bit VST3 audio effect**, not a MIDI instrument.
Stereo, mono, and mono-input/stereo-output host layouts are supported.
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
2. Confirm **VST3 0.1.4** is shown in the editor header, so Cakewalk isn't still
   loading the old module. The window is now 560 × 420 logical pixels and capped
   at 600 physical pixels high when the host requests DPI scaling. Run, meters,
   and audition controls stay above the code area. The scrollbar appears only
   when the actual code extends beyond the viewport; short programs do not reserve
   a large blank scrolling area. Long programs can be scrolled by dragging the
   scrollbar or using the mouse wheel while the pointer is over the editor.
3. Turn down monitoring, start playback, unbypass the effect, and click **Test tone**.
   It automatically submits:

   ```text
   o: sin 440 >> mul 0.05;
   ```

   Route the track output to the master; MIDI notes and input audio are not
   required for this oscillator. **Mute** submits a silent program without
   discarding the editor draft. **Restore code** restores the pre-tone draft;
   it may produce audio again if the restored program is a generator.
4. Click **Pass input** and route a known clip/live audio into the effect:

   ```text
   o: ~input;
   ```

   This is unity gain. The original default program, `o: ~input >> mul 0.1;`,
   still requires incoming audio and is 20 dB quieter. For live input in Cakewalk,
   select the audio device input and enable **Input Echo**. MIDI input alone is
   not audio input for this effect.
5. Test host buffers 32, 64, 128, 192 if available, 256 and 512 at 44.1/48 kHz.
   Audio should continue after the initial 128-sample delay.
6. Close/reopen the editor several times, type and Run again, and click back
   into the DAW to confirm it can regain keyboard focus. Check Cakewalk's plugin
   window keyboard-input control if it intercepts shortcuts.
7. Run a changed program, save the project, close/reopen the project and confirm
   the submitted program and audio are restored.
8. Keep a VST2 instance on a different track and confirm both formats are usable.

## If it is still silent

- **No audio callbacks:** the host is not running this instance. Start transport,
  enable the audio engine/track/FX rack, and check bypass. For live input, enable
  Input Echo. The plugin cannot make Cakewalk call an instance that is suspended.
- **Audio running; IN zero:** no audio is arriving. Test tone doesn't need input;
  Pass input does. Check the track's audio clip/device input and Input Echo.
- **Audio running; OUT nonzero, but nothing audible:** audio is leaving the
  plugin. Check track/master mute, faders, output routing, audio device, and
  monitoring. Confirm the track isn't routed to an unused hardware output.
- **Audio running; OUT zero with Test tone:** look for the visible **Glicol error**
  message and confirm you loaded version 0.1.4. Report the message plus the
  IN/OUT values and whether transport is running.

Run applies on the next engine block instead of waiting for a musical bar.
The VST3 wrapper clears stale output-silence flags, and the plugin declares an
infinite tail because user code may generate sound on silent input. This keeps
it eligible for processing rather than declaring itself finished at silence;
the host still controls transport and audio processing.

When upgrading, close Cakewalk, back up the old VST3 bundle **outside** scanned
folders, replace the whole installed VST3 directory, rescan, and reopen. The
class ID is unchanged so projects can still identify the same VST3 plugin.

## Enter code, not example labels

`Tone test:` is explanatory text, not a Glicol statement. A program such as
`o: ~input >> mul 0.1;` followed by `Tone test: o: sin 440 >> mul 0.1;`
causes a syntax error on the label's line. Replace the complete editor contents
with one of the valid code examples above, then click Run. Do not paste Markdown
backticks or labels into the editor. The default program now contains code only;
the Test tone control supplies the oscillator without requiring a pasted label.

## First-open position and late display scaling

The user reported that the editor initially appears down and to the right, but
looks correct after closing and reopening. The fixed-scale GUI previously
rejected a display-scale request made after attaching its window. Version 0.1.3
handles that ordering by recreating the Windows child at the requested scale
and requesting the matching physical size from the host, with origin `(0, 0)`.
Draft text and the Restore code buffer survive the recreation without changing
the submitted audio program. Repeated identical scale requests do not rebuild.

This is a targeted fix for a confirmed initialization-order limitation; the
actual Cakewalk displacement has not been reproduced in the native tests.
For acceptance, insert a fresh instance and check the **first** opening before
closing it. Compare against reopening, and verify code, buttons and mouse clicks
are aligned at the current Windows display scale. Also try reopening a saved
project. If the first view is still displaced, provide first-open and reopened
screenshots, display scaling, and whether the window is on the primary monitor.

### VSTHost first-load follow-up

The user confirmed that 0.1.3 still opens displaced on the first load in
VSTHost. Its scaling-order patch is **not** a confirmed fix for that symptom.
Version 0.1.4 removes a separate host-wide side effect in the pinned baseview
backend: an embedded window no longer calls `SetProcessDpiAwarenessContext`
after creating its HWND and renderer. A plugin must inherit its host's display
policy, not alter the whole process on its first opening.

Completely exit VSTHost/Cakewalk before installing and start a fresh host
process afterward. Merely closing/reopening the editor cannot reset process
DPI policy already changed by an older loaded DLL. Check the very first editor
opening in the new process; actual OpenGL positioning still requires host
confirmation. The native tests check real Win32 child origin, size and unchanged
DPI context on opening/reopening, but do not render the actual OpenGL editor.

## Save and load plain text programs

- **Save text** opens a Windows Save As dialog and writes the current editor
  contents, including an unsubmitted draft, as UTF-8 text. The default extension
  is `.txt`; `.glicol` and other explicitly chosen extensions also work.
- **Load text** opens a Windows file chooser. If current text has not been saved
  to disk or differs from the last loaded/saved text, choose Save first, replace
  without saving, or Cancel. Cancellation and read errors leave current text
  unchanged.
- Loading fills the editor only. Click **Run** to apply the loaded program.
  Loading/saving does not change the audio program by itself.
- Save asks before overwriting an existing file. It stages the complete write
  in the destination folder and atomically replaces the file; a failed save
  does not truncate the old program. Errors and success are shown in the editor.
- Unicode filenames, spaces and CRLF line endings are preserved. UTF-8 BOM
  input is accepted. UTF-16/binary files are rejected explicitly; resave those
  as UTF-8 in a text editor. The maximum program file size is 1 MiB.
- File dialogs run outside the borrowed rendering callback, so their nested
  Windows message loop can repaint safely. No disk I/O runs in the audio
  callback and the host's current working directory is not changed.

## Scope and licensing

The original MIT license is retained. The VST3 wrapper uses the ISC-licensed
nice-plug framework and MIT/Apache-2.0 `vst3` bindings. Dependency notices are
provided with release packages.

No MIDI support or VST2-to-VST3 saved-project migration is added.
The Glicol engine may allocate while rendering/compiling code; this is not a
claim of allocation-free realtime processing. Engine-reported errors are now
shown in the editor instead of being discarded. The last error stays visible
until different code is submitted; the underlying engine determines whether a
previous graph can continue running after a compile error.
