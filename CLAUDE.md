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

**Notable CLI args** (full list in `cli.rs`): `--object` (face/head/ball/person/car/...), `--device` (`cpu:0` default, `cuda:0`, `coreml`, `trt:0`), `--scale` (n/s/m/l), `--ver` (model version), `--output-filepath` (copies final video out of `runs/`), `--add-captions`, `--keep-text`/`--prioritize-text`, `--min-area-ratio` (default `0.05`; drops detections smaller than this fraction of the largest detection's area so incidental faces—e.g. on a book cover—don't inflate the object count into a subject-splitting stacked crop; `0` disables, ball-type objects exempt; see `filter_small_relative_objects` in `video_processor_utils.rs`).

## Architecture

Landscape-to-portrait (9:16) video converter: YOLO object detection → crop calculation → smoothed frame output.

**Pipeline:** `main.rs` → `cli.rs` → `config.rs` → VideoProcessor loop (`crop.rs` + smoothing) → optional audio/captions (`audio.rs`, `transcript.rs`).

**Three VideoProcessor implementations** (strategy pattern, trait in `video_processor.rs`):
- `HistorySmoothingVideoProcessor` — default, history-based interpolation
- `SimpleSmoothingVideoProcessor` — `--use-simple-smoothing`, previous-frame-only comparison
- `BallVideoProcessor` — auto-selected for `--object ball`, 3-frame prediction

**`crop.rs`** is the most complex module (~500 lines). Logic branches by object count: 0→centered 3:4, 1→centered on object, 2→single or stacked 9:8, 3→special 9:6+9:10 stacking for equally-spaced heads, 6+→largest object.

**Key modules:** `image.rs` (cut detection via image similarity), `history.rs` (frame/crop history), `video_processor_utils.rs` (shared helpers), `video_sink.rs` (output encoding + fps probe), `config.rs` (maps CLI args to ONNX model paths in `model/`).

**Output encoding (`video_sink.rs`):** The usls `Viewer` auto-generates output paths and has no save-path API, so `VideoSink` drives a `video-rs` `Encoder` directly to write the cropped frames to the exact `processed_video.mp4` path `main.rs` expects (and later copies to `--output-filepath`). The usls `DataLoader` no longer exposes the source frame rate, so `probe_fps` shells out to `ffprobe` (falls back to 30 fps); this fps drives both smoothing math and output frame timing.

## Build Notes

- `build.rs` sets macOS `-fapple-link-rtlib` linker flag
- `usls` from upstream `jamjamjon/usls` (pinned rev), `video`+`viewer` features; **device features are platform-gated** in `Cargo.toml` via `[target.'cfg(...)']` — `coreml` on macOS, `cuda`+`tensorrt` on Linux (so the Docker/Cloud Run build needs no Cargo.toml patching)
- `video-rs` 0.11.0 from crates.io (used directly for output encoding; no `[patch.crates-io]`)
- `ffmpeg-next` may need pinning to match the locally installed ffmpeg (e.g. `cargo update -p ffmpeg-next --precise 8.1.0` for system ffmpeg 8.1.x), else its non-exhaustive enum matches fail to compile
- Output goes to `runs/YYYYMMDD_HHMMSS_ffffff/`

## Deployment (Cloud Run GPU + TensorRT)

`deploy/` runs the converter as a Cloud Run Job on an NVIDIA L4 GPU with the TensorRT execution provider (`--device trt:0`).

- **`Dockerfile.gcloud`** — 3-stage `linux/amd64` build: pulls TensorRT 10 libs from the NGC container, builds the binary on `cuda:12.6.3-cudnn-devel-ubuntu24.04`, ships on the matching `runtime` image. The Ubuntu 24.04 base (glibc 2.39) is required — `ort`'s prebuilt ONNX Runtime references glibc ≥2.38 symbols (`__isoc23_*`), so the older ubuntu22.04/glibc 2.35 base failed to link. No `Cargo.toml` patching is needed: device features are platform-gated (see Build Notes), so the Linux build picks up `cuda`+`tensorrt` and never pulls `coreml`. Copies ONNX Runtime provider `.so`s, TensorRT libs, and `model/` into the image.
- **`entrypoint.sh`** — symlinks `/root/.cache/usls/caches/tensorrt` → `/data/cache/tensorrt` (GCS FUSE) so the (slow-to-build) TensorRT engine cache persists across executions. Only this cache is symlinked — model downloads stay on local disk to avoid cross-device rename errors.
- I/O is via a GCS bucket (`<project-id>-land2port`) mounted at `/data`, with `input/` and `output/` folders. Jobs read `/data/input/X.mp4` and write `--output-filepath /data/output/X.mp4`.

**Workflow scripts** (each reads project/region from `gcloud config`):
- `deploy-gcloud.sh [job-name]` — one-time: enable APIs, create Artifact Registry repo, build+push, create the Job with GPU + GCS volume, grant storage IAM.
- `update-gcloud.sh [job-name]` — rebuild + push + update the Job's image (use after code changes).
- `upload-video.sh <file>` / `download-video.sh <file>` — move videos in/out of the bucket.
- `run-job.sh <file> [opts]` — execute the job on an uploaded video; wraps converter flags into the `--args` list.
