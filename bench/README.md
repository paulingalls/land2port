# Benchmark harness

Measures how each performance change affects land2port, and verifies the
output hasn't regressed, using one fixed input video across runs.

The binary itself records per-stage timings (`decode`, `detect`, `ocr`,
`cut_detect`, `crop_render`, `encode_write`, `encode_finalize`, plus the
audio/caption stages) and writes them to `metrics.json` at the end of every
run — locally into the `runs/<timestamp>/` directory, and additionally next
to the delivered video (`<output>.metrics.json`) when `--output-filepath`
is set, so Cloud Run executions land their metrics in the GCS bucket.

## Workflow

1. **Capture a baseline** on the current code before changing anything:

   ```bash
   # local (use a GPU/accelerator device if you have one; keep args identical
   # across runs — e.g. --device coreml on macOS, --device cuda:0 on Linux)
   ./bench/run-bench.sh baseline ./video/bench-input.mp4 --device coreml

   # or on the real Cloud Run job (input uploaded via deploy/upload-video.sh)
   ./bench/run-bench-cloud.sh baseline bench-input.mp4
   ```

2. **Make a change**, then run it under a new label with the *same* input
   and args:

   ```bash
   ./bench/run-bench.sh fast-cut-detect ./video/bench-input.mp4 --device coreml
   ```

3. **Compare** timings and verify reliability:

   ```bash
   ./bench/compare.sh baseline fast-cut-detect
   ```

   This prints wall time, throughput (fps), and per-stage deltas, then runs
   reliability checks of the candidate output against the baseline output:

   - output exists and is non-empty
   - resolution and frame rate unchanged
   - frame count within ±2 (`BENCH_FRAME_TOL`)
   - duration within 0.2s (`BENCH_DURATION_TOL`)
   - audio stream count unchanged
   - mean SSIM vs baseline >= 0.95 (`BENCH_SSIM_MIN`)
   - worst-frame SSIM >= 0.85 (`BENCH_SSIM_MIN_FRAME`) — catches a localized
     glitch the mean would hide; the offending frame number is reported
   - mean PSNR (dB) is logged as an informational signal (not gated)

   Per-frame SSIM is written to `results/<candidate>/ssim-frames.log` and the
   raw ffmpeg quality output to `ffmpeg-quality.stderr`, so you can inspect or
   plot the full curve after a run.

   The script exits non-zero if any check fails, so it can gate CI.

## Notes

- Results live in `bench/results/<label>/` (`output.mp4`, `metrics.json`,
  `meta.txt`, `run.log`). The directory is git-ignored; labels are
  append-only — the scripts refuse to overwrite an existing label.
- `meta.txt` records the git SHA, input hash, and args of each run;
  `compare.sh` warns when two runs being compared used different inputs
  or args.
- SSIM threshold guidance: a pure performance refactor should score
  ~0.99+. A deliberate quality trade-off (e.g. swapping the resize filter)
  will land lower — eyeball the output and decide; 0.95 is the default
  floor for "no visible regression".
- Crop *decision* changes (different smoothing/cut behavior) show up as
  SSIM drops even when each frame is sharp. If a change is *supposed* to
  alter crop decisions, expect the SSIM gate to fail and review manually.
- Cloud runs write to `gs://<bucket>/output/bench/<label>/` so they never
  clobber regular job output. First cloud run after a model/dtype change
  rebuilds the TensorRT engine — discard that run and benchmark the second
  execution, which uses the cached engine.
- `compare.sh` needs `ffmpeg`/`ffprobe` on PATH.
