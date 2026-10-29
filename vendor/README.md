# gpui's Linux and wgpu backends, vendored

`gpui-pre-linux` and `gpui-pre-wgpu` 0.3.9, as crates.io publishes them,
taken in through `[patch.crates-io]` in the top `Cargo.toml`. The first
commit that touches them adds them unchanged, without their `Cargo.lock`
and the wgpu backend's benchmark, so `git log -p -- vendor` shows every
change made since; in the code, `gpui-greeter:` comments mark them. Both
crates are Zed's, under the Apache License 2.0, whose text is beside each.

To take a newer gpui, copy its crates over these, drop their `Cargo.lock`
and benchmarks, and carry the changes over.
