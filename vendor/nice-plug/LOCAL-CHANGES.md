# Local VST3 host-compatibility patch

Source: https://github.com/AlexCharlton/nice-plug
Revision: `50fed0ebabc3b880015d2a06e9436e4220b548f6`
Original ISC license is retained as `LICENSE`.

Only the four required framework crates are vendored. The workspace manifest
is reduced to these crates; their source and versions otherwise match upstream.
The crate manifests declare their workspace explicitly and the parent plugin
workspace excludes them, so Cargo resolves their inherited metadata consistently.

The VST3 processor wrapper clears each output bus's `silenceFlags` on a
successful process call. Upstream left these flags untouched, so an output bus
marked silent by the host could remain marked silent even when the plugin
generated nonzero audio. Clearing the flags is conservative: hosts inspect the
actual output samples rather than incorrectly skipping them.

The plugin regression test pre-fills output silence flags and verifies they are
cleared after processing. This is not a claim that Cakewalk's actual no-sound
report has been reproduced or resolved.
