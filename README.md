# Glicol VST — Windows VST3 edition

A separate MIT-licensed VST3 port of the Glicol stereo code-based audio effect.

- Product: **Glicol VST**
- Manufacturer: **Dexter U.S.**
- Format: **VST3**, 64-bit Windows, mono/stereo audio effect, no MIDI
- Latency: **128 samples**, declared to the host
- UI: compact, DPI-height-capped window with a visibly scrollable code editor
- Audio checks: **Test tone**, **Mute**, **Restore code**, **Pass input**, IN/OUT meters,
  callback activity, and visible engine errors
- Host compatibility: mono negotiation, immediate Run, generator keep-alive,
  and output-silence flag handling
- State: the last submitted program is persisted in VST3 projects/presets

See [WINDOWS-VST3.md](WINDOWS-VST3.md) for build, installation, limitations,
and actual Windows/Cakewalk acceptance checks.

The VST2 edition remains unchanged on the private repository's `main` branch.
This source is maintained separately on the `vst3` branch. It uses the existing
Windows build workflow without changing its authorization-sensitive file.
The workflow's raw `glicol_vst.dll` artifact is a **VST3 module** on this branch;
it must be packaged into `Glicol VST.vst3/Contents/x86_64-win/Glicol VST.vst3`.
Use the installation ZIP rather than dropping that CI DLL into a VST2 folder.

The source is based on the original MIT-licensed
[Glicol VST](https://github.com/glicol/glicol-vst). No upstream repository was changed.
The VST3 integration uses the ISC-licensed nice-plug framework and permissively
licensed VST3 bindings. Native regression tests are not proof of host keyboard
behavior: complete the real-host checklist before considering it confirmed.
The framework is vendored at its original pinned revision with a small
[documented silence-flag patch](vendor/nice-plug/LOCAL-CHANGES.md).
