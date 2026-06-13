# Code Review Findings — Backlog

Source: a multi-agent code review of land2port (5 reviewers across crop geometry,
the VideoProcessor strategies, the output pipeline, performance, and Rust
idioms/robustness; 54 raw findings synthesized and prioritized).

The high-value **robustness + per-frame allocation** cluster has already been
addressed (see commit "Harden output pipeline + cut per-frame allocations"):
encoder finalize-on-drop, fps-probe fallback surfacing, crop-rect clamping,
even output dims, three-head NaN/order safety, source validation, `into_rgb8`/
`crop_imm`, cut-detection borrow, headless `Cow`, and `VecDeque` history.

This document tracks the **remaining** findings — mostly design/quality and
lower-leverage performance — that were intentionally left out of that pass.
Line references are approximate (the codebase shifted after the first pass);
treat the module/function as authoritative.

> Note on performance: profiling showed the pipeline is **video-decode-bound**
> (~37s of a ~40s run is ffmpeg decode inside the usls `DataLoader`), and a
> VideoToolbox hardware-decode spike was a net loss because the DataLoader
> already pipelines software decode on a background thread. So the performance
> items below reduce CPU/allocator pressure but will **not** move wall-clock on
> this workload — pursue them for cleanliness/memory, not speed.

---

## Theme: Smoothing strategy design & coupling

### Don't let `--smooth-duration 0` silently disable Ball/Simple smoothing
- **Impact:** medium · **Effort:** medium · **Kind:** robustness
- **Where:** `video_processor.rs` (the `smooth_duration_frames > 0` gate in `process_video`)
- **Why:** the loop only calls `process_frame_with_smoothing` when
  `smooth_duration_frames > 0`, but the Simple and Ball processors ignore that
  value entirely (underscored param). So `--smooth-duration 0` silently turns off
  the ball's 3-frame prediction and simple previous-frame comparison, neither of
  which conceptually depends on a duration.
- **Suggestion:** always call `process_frame_with_smoothing` and let the History
  processor degenerate to passthrough internally when `smooth_duration_frames == 0`,
  or move the "no smoothing" fast path into the History processor only.

### Interpolate all four crop fields, not just X
- **Impact:** medium · **Effort:** medium · **Kind:** bug
- **Where:** `video_processor_utils.rs` `interpolate_crop_results` (Single→Single)
- **Why:** only `x` is eased; `y`/`width`/`height` jump to the destination at
  t=0, so any transition that changes height/width/vertical framing shows a
  visible vertical/zoom pop at the start of an otherwise-smooth pan.
- **Suggestion:** interpolate `x, y, width, height` with the same `t`. If freezing
  was deliberate for stability, gate it — only freeze a field when start and dest
  differ by less than the smooth threshold, so genuine size/vertical changes ease too.

### Ease toward the latest crop in `finalize` instead of holding the stale crop
- **Impact:** medium · **Effort:** medium · **Kind:** design
- **Where:** `history_smoothing_video_processor.rs` `finalize_processing`
- **Why:** trailing buffered frames are flushed using `prev_crop`, freezing the
  end of the video on the pre-transition framing (the frames were buffered
  *because* a transition was pending). Worse, if `previous_crop` is `None` at end,
  buffered frames are dropped entirely — output shorter than source, desyncing
  the audio mux under `--add-captions`.
- **Suggestion:** interpolate from `prev_crop` toward the most recent `latest_crop`
  over the remaining frames (reuse `process_history_with_interpolation`), and
  flush remaining history even when `previous_crop` is `None` (fall back to a
  centered/identity crop) so output frame count == input frame count.

### Pull shared smoothing state into a common struct/helper
- **Impact:** medium · **Effort:** medium · **Kind:** design
- **Where:** the three processor structs (`history_/simple_/ball_…`)
- **Why:** all three independently declare `previous_crop`; two declare a
  "previous image" field with inconsistent names (`last_image` vs
  `most_recent_image`) plus a `CutDetector`. The end-of-frame update (set
  previous_crop, set previous image, write frame) is copy-pasted with naming
  drift, so fixes must be remembered in each.
- **Suggestion:** introduce a small `SmoothingState { previous_crop, previous_image }`
  (optionally the `CutDetector`) embedded in each processor, or a default-provided
  trait helper, so the strategies differ only in the crop-selection decision.

---

## Theme: Memory & lower-leverage performance

### Don't store full-resolution frames in `CropHistory`
- **Impact:** medium · **Effort:** large · **Kind:** design
- **Where:** `history.rs` `FrameData`; `history_smoothing_video_processor.rs` (the `img.clone()` into history)
- **Why:** each `FrameData` holds a full owned `Image`, so up to
  `smooth_duration_frames` (~30–60) full-res frames live in RAM (~180–360 MB at
  1080p, 4× at 4K), each entering via a clone and later re-cloned in
  `create_cropped_image`. Only the `CropResult` actually needs buffering for the
  smoothing decision. **This is the dominant peak-RSS contributor.**
- **Suggestion:** decouple the smoothing decision (a cheap `CropResult` ring) from
  pixel storage — buffer only crop decisions and stream frames through a bounded
  ring, or store `Arc<Image>` so frames are shared rather than deep-copied. At
  minimum, move (not clone) the owned current `Image` into history.

### Run cut detection on a downscaled thumbnail
- **Impact:** medium · **Effort:** medium · **Kind:** performance
- **Where:** `image.rs` `CutDetector::is_cut`
- **Why:** `rgb_hybrid_compare` runs an SSIM-style structural+color compare over
  the entire full-res frame every frame just to produce one "cut?" scalar — an
  O(w·h) pass that scales with resolution. The decision is robust to heavy downscaling.
- **Suggestion:** maintain a small thumbnail (e.g. 256×144) per frame and compare
  thumbnails; store only the thumbnail as "previous" (also shrinks the
  `last_image` cost). ~50–100× cheaper compare, no material decision change.

### Run the OCR/text model batched instead of per-frame with a clone
- **Impact:** medium · **Effort:** medium · **Kind:** performance
- **Where:** `video_processor.rs` (the `keep_text`/`prioritize_text` block)
- **Why:** with `prioritize_text`, `text_model.forward(&[image.clone()])` runs
  every frame — a full-frame clone plus a separate unbatched inference, sequential
  with YOLO (which is already batched).
- **Suggestion:** when text handling is active, run the OCR model over the same
  batch the frame source yields (`text_model.forward(&images)`) once per batch,
  and pass a borrowed image rather than cloning.

### Default per-frame resize to a cheaper filter; Lanczos3 behind `--high-quality`
- **Impact:** medium · **Effort:** medium · **Kind:** performance
- **Where:** `image.rs` (the `resize(...)` calls in `create_cropped_image`)
- **Why:** every emitted frame goes through `Lanczos3`, the slowest filter in the
  `image` crate; the quality difference vs Triangle/CatmullRom is usually
  imperceptible in motion video.
- **Suggestion:** default to `Triangle`/`CatmullRom`, gate `Lanczos3` behind an
  opt-in `--high-quality`, or use `fast_image_resize` (SIMD) for a large speedup
  at equivalent quality. Benchmark on a representative clip.

### Cache the `RUST_LOG` debug check once
- **Impact:** low · **Effort:** small · **Kind:** performance
- **Where:** `video_processor_utils.rs` `is_debug_enabled` (~30 call sites)
- **Why:** every `debug_println` does `env::var("RUST_LOG")` (getenv + String
  alloc) plus `.to_lowercase()` (another alloc), even when debug is off; several
  fire per frame.
- **Suggestion:** cache the boolean once via `OnceLock`/`LazyLock`, or switch to
  `log`/`tracing` which compile the level check to a cheap atomic and format lazily.

---

## Theme: Validation, tests & correctness polish

### Validate CLI numeric arguments
- **Impact:** medium · **Effort:** small · **Kind:** robustness
- **Where:** `cli.rs` numeric args; checked early in `main`
- **Why:** `smooth_percentage`, `object_prob_threshold`, `cut_similarity`,
  `cut_start`, `text_area_threshold`, `smooth_duration` accept any value with no
  range checks. A negative/>1.0/NaN probability silently disables or breaks
  detection/smoothing with no feedback.
- **Suggestion:** add a `validate()` on `Args` (or checks early in `main`)
  bounding probabilities/similarities to `[0,1]`, requiring non-negative
  durations/percentages, rejecting NaN, bailing with a clear message.

### Add unit tests for the remaining correctness-critical logic
- **Impact:** medium · **Effort:** medium · **Kind:** robustness
- **Where:** `image.rs` `is_cut` thresholds; `history_smoothing_video_processor.rs`
  `select_closest_crop`/interpolation-length decision tree
- **Why:** `parse_frame_rate`, `clamp_crop_rect`, and the three-head selection now
  have tests, but the three-threshold cut decision and the crop-selection decision
  tree (incl. the `smooth_duration_frames / 4` integer-division edge that silently
  becomes 0 for durations < 4 frames) are still untested.
- **Suggestion:** add focused tests for `is_cut` threshold transitions and the
  `crop_to_use` branches, including the `/4` truncation edge (use
  `(smooth_duration_frames / 4).max(1)` or float math).

### Use `highest_confidence_ball.clone()` instead of rebuilding via `from_cxcywh`
- **Impact:** low · **Effort:** small · **Kind:** robustness
- **Where:** `ball_video_processor.rs` (multi-ball path)
- **Why:** the single-ball path stores `objects[0].clone()`, but the multi-ball
  path reconstructs the `Hbb` via `Hbb::from_cxcywh(...)`, dropping
  confidence/class metadata and converting through center coords.
  `predict_current_hbb` keys off `xmin`/`ymin`, so mixing a `from_cxcywh`-derived
  box with directly-cloned boxes across frames can inject rounding noise into the
  velocity/acceleration prediction.
- **Suggestion:** store `highest_confidence_ball.clone()` (same as the single-ball path).

---

## Theme: Clarity & dead code

### Encode stacked-layout intent in the type instead of float aspect sniffing
- **Impact:** medium · **Effort:** medium · **Kind:** design
- **Where:** `image.rs` (stacked-crop layout selection) ↔ `crop.rs` (stack geometry)
- **Why:** the renderer re-derives the three-head 9:6 / 9:10 case by comparing
  aspect ratios against `1.5`/`0.9` with ±0.1 tolerance, coupling `image.rs` to
  magic constants in `crop.rs`. If crop geometry shifts slightly, the renderer
  silently falls into the equal-half-height default and stacks wrong. The layout
  is known at crop-calc time but thrown away and reverse-engineered.
- **Suggestion:** add a `StackLayout` enum to the `Stacked` `CropResult` variant
  (e.g. `Equal | TopWideBottomNarrow`) so the renderer matches on intent.

### Name the `0.15` always-cut magic number and fix stale docs
- **Impact:** low · **Effort:** small · **Kind:** design
- **Where:** `image.rs` `CutDetector`
- **Why:** `always_cut_threshold = 0.15` is hardcoded alongside the configurable
  thresholds, and the `CutDetector::new` doc-comment still claims defaults of
  0.15/0.7 that don't match the real CLI defaults (`cut_similarity` 0.4 /
  `cut_start` 0.8).
- **Suggestion:** promote `0.15` to a named const (or constructor param), update
  the doc-comment to the real defaults, and document how the three thresholds interact.

### Factory for processor selection; rename `viewer` → `sink`
- **Impact:** low · **Effort:** small · **Kind:** simplification
- **Where:** `main.rs` (processor selection); `video_processor.rs` (the `viewer` binding/params)
- **Why:** processor selection is a three-arm `if/else` each duplicating
  `processor.process_video(&args, &processed_video)?`. Separately, after the usls
  migration the encoder/sink is still named `viewer` everywhere, obscuring its
  primary encoder role.
- **Suggestion:** add `fn make_processor(args) -> Box<dyn VideoProcessor>` and call
  `process_video` once; rename the `viewer` binding and trait params to `sink`.

### Clean up the lazy-encoder access and dead audio code
- **Impact:** low · **Effort:** small · **Kind:** simplification
- **Where:** `video_sink.rs` (encoder `expect`); `audio.rs` (`border_style` match, stray `println!`)
- **Why:** `write_frame` inits the encoder then re-accesses it with
  `.as_mut().expect("…")` — a latent panic on a self-maintained invariant.
  Separately, `audio.rs` collapses three `border_style` arms to the same value and
  unconditionally `println!`s `filter_str` on every caption burn (debug noise).
- **Suggestion:** bind the encoder via a single `get_or_insert`-style path so
  there's one access and no `expect`; simplify `border_style` and route
  `filter_str` through `debug_println` (or remove it).

### Make OCR/audio failure modes clearer
- **Impact:** medium · **Effort:** small · **Kind:** robustness
- **Where:** `audio.rs` `extract_audio`; `config.rs` `get_model_path`
- **Why:** `extract_audio` uses `-acodec copy` into an `.mp4` container; for many
  source codecs, or a source with no audio stream, this fails cryptically or
  yields a file that breaks `combine_video_audio`. Separately, `get_model_path`
  silently falls back to the default model for an unsupported `--ver`/`--scale`.
- **Suggestion:** `ffprobe` for an audio stream first and either skip captioning
  gracefully or bail clearly; in `get_model_path`, emit a warning when falling
  back to the default model.
