# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --release        # Always use --release
cargo test
cargo run --release -- --source ./video/input.mp4 --headless
RUST_LOG=debug cargo run --release -- ...   # Debug output
```

**Prerequisites:** Rust (edition 2024), ffmpeg, `OPENAI_API_KEY` (for `--add-captions`).

## Architecture

Landscape-to-portrait (9:16) video converter: YOLO object detection → crop calculation → smoothed frame output.

**Pipeline:** `main.rs` → `cli.rs` → `config.rs` → VideoProcessor loop (`crop.rs` + smoothing) → optional audio/captions (`audio.rs`, `transcript.rs`).

**Three VideoProcessor implementations** (strategy pattern, trait in `video_processor.rs`):
- `HistorySmoothingVideoProcessor` — default, history-based interpolation
- `SimpleSmoothingVideoProcessor` — `--use-simple-smoothing`, previous-frame-only comparison
- `BallVideoProcessor` — auto-selected for `--object ball`, 3-frame prediction

**`crop.rs`** is the most complex module (~500 lines). Logic branches by object count: 0→centered 3:4, 1→centered on object, 2→single or stacked 9:8, 3→special 9:6+9:10 stacking for equally-spaced heads, 6+→largest object.

**Key modules:** `image.rs` (cut detection via image similarity), `history.rs` (frame/crop history), `video_processor_utils.rs` (shared helpers), `config.rs` (maps CLI args to ONNX model paths in `model/`).

## Build Notes

- `build.rs` sets macOS `-fapple-link-rtlib` linker flag
- `usls` dependency from GitHub with `coreml`/`cuda`/`tensorrt` features
- Patched `video-rs` fork via `[patch.crates-io]`
- Output goes to `runs/YYYYMMDD_HHMMSS_ffffff/`
