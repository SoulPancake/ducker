# DUCKER

Standalone real-time sidechain ducker desktop app written in Rust using `cpal` + `egui`/`eframe`.

## Build

```bash
cargo clippy --all-targets --all-features
cargo build --release
```

## Routing with Audient EVO 4

1. Select **EVO 4 Input** as the app input device.
2. Set **Main Ch** to `1` (input index `0`).
3. Set **SC Ch** to `2` (input index `1`).
4. Select **EVO 4 Output** as the output device.

This maps hardware Input 1 as program audio and Input 2 as the sidechain trigger.

## BlackHole tip

If you want to duck software audio (DAW, browser, or stream), install **BlackHole 2ch** and select it as the input routing source for the sidechain channel.
