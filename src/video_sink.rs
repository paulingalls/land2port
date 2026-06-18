use crate::metrics;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::Instant;
use usls::{Image, Key, Viewer};
use video_rs::{Encoder, Frame, Time, encode::Settings};

/// One encode request: the cropped frame's RGB24 bytes plus its dimensions.
struct EncodeMsg {
    w: usize,
    h: usize,
    data: Vec<u8>,
}

/// Sink for processed frames.
///
/// Wraps a usls [`Viewer`] (for the optional preview window) and a video-rs
/// [`Encoder`] (for writing the cropped output to a deterministic path). The
/// usls `Viewer` auto-generates output paths and exposes no save-path API, so
/// we drive the encoder ourselves to keep writing to the path `main.rs` expects.
///
/// Encoding runs on a dedicated thread fed by a bounded FIFO channel, so the
/// H.264 encode overlaps the crop-render and detection work on the main thread
/// instead of running serially after each frame. Output is byte-identical to an
/// inline encode: the channel is FIFO and single-consumer, so frames are encoded
/// in exactly the order produced, with the same per-frame timestamps.
///
/// The `video-rs` `Encoder` wraps non-`Send` ffmpeg state, so it is constructed
/// and owned entirely inside the encoder thread — only plain frame bytes (which
/// are `Send`) cross the channel.
pub struct VideoSink {
    viewer: Viewer<'static>,
    tx: Option<SyncSender<EncodeMsg>>,
    handle: Option<JoinHandle<Result<()>>>,
    frame_index: usize,
}

impl VideoSink {
    /// Creates a sink that encodes to `saveout` at the given frames-per-second.
    pub fn new(saveout: impl Into<PathBuf>, fps: f64) -> Self {
        let saveout = saveout.into();
        // Bounded so a slow encoder applies backpressure rather than letting
        // in-flight frames (each ~6 MB at 1080x1920) grow unbounded in RAM.
        let (tx, rx) = sync_channel::<EncodeMsg>(8);

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut encoder: Option<Encoder> = None;
            let mut frame_index: usize = 0;

            while let Ok(msg) = rx.recv() {
                let start = Instant::now();
                if encoder.is_none() {
                    // The encoder is created lazily from the first frame's
                    // dimensions, mirroring how the usls `Viewer` initializes
                    // encoding.
                    let settings = Settings::preset_h264_yuv420p(msg.w, msg.h, false);
                    encoder = Some(
                        Encoder::new(saveout.clone(), settings).context("creating video encoder")?,
                    );
                }
                let enc = encoder.as_mut().expect("encoder initialized above");
                let frame = Frame::from_shape_vec((msg.h, msg.w, 3), msg.data)
                    .context("building encoder frame")?;
                // Output frame timing is derived from a monotonic frame counter
                // at the source fps, matching the old `Viewer::with_fps`.
                let timestamp = Time::from_secs_f64(frame_index as f64 / fps);
                enc.encode(&frame, timestamp).context("encoding video frame")?;
                frame_index += 1;
                metrics::record("encode_write", start.elapsed());
                metrics::inc("frames_written", 1);
            }

            // Sender dropped → no more frames; finalize the container (the mp4
            // moov-atom write/seek happens here).
            if let Some(mut enc) = encoder.take() {
                let start = Instant::now();
                enc.finish().context("finalizing video encoder")?;
                metrics::record("encode_finalize", start.elapsed());
            }
            Ok(())
        });

        Self {
            viewer: Viewer::default().with_window_scale(0.5),
            tx: Some(tx),
            handle: Some(handle),
            frame_index: 0,
        }
    }

    /// Polls the preview window for a key press.
    pub fn wait_key(&mut self, delay_ms: u64) -> Option<Key> {
        self.viewer.wait_key(delay_ms)
    }

    /// True once the preview window has been opened and then closed by the user.
    pub fn is_window_exist_and_closed(&self) -> bool {
        self.viewer.is_window_exist_and_closed()
    }

    /// Displays (unless headless) and enqueues one output frame for encoding,
    /// consuming it.
    ///
    /// Takes the cropped image by value so the pixel buffer can be moved into the
    /// encode message (`into_rgb8`) rather than cloned. `imshow` stays on the
    /// calling (main) thread — required on macOS, where window operations must
    /// not run on a background thread — while only the plain RGB bytes cross to
    /// the encoder thread. Blocks if the encoder is more than the channel bound
    /// behind.
    pub fn write_frame(&mut self, img: Image, headless: bool) -> Result<()> {
        if !headless {
            self.viewer.imshow(&img)?;
        }

        let rgb = img.into_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        let data = rgb.into_raw();

        self.tx
            .as_ref()
            .expect("write_frame after finalize")
            .send(EncodeMsg { w, h, data })
            .map_err(|_| anyhow::anyhow!("encoder thread terminated early"))?;
        self.frame_index += 1;
        Ok(())
    }

    /// Number of frames enqueued for encoding so far.
    pub fn frame_count(&self) -> usize {
        self.frame_index
    }

    /// Flushes and finalizes: drops the sender so the encoder thread drains its
    /// queue and writes the container trailer, then joins and propagates any
    /// encode error.
    pub fn finalize(&mut self) -> Result<()> {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("encoder thread panicked"))??;
        }
        Ok(())
    }
}

impl Drop for VideoSink {
    fn drop(&mut self) {
        // Ensure the mp4 is finalized (moov atom written, packets flushed) even
        // if the processing loop exits early via an error or panic before the
        // explicit finalize() runs. A successful finalize() already took the
        // sender and handle, so this is a no-op on the happy path. Errors can't
        // propagate out of drop, so they are logged.
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(Err(err)) => {
                    eprintln!("warning: failed to finalize video output on drop: {err}")
                }
                Err(_) => eprintln!("warning: encoder thread panicked during drop"),
                Ok(Ok(())) => {}
            }
        }
    }
}

const DEFAULT_FPS: f64 = 30.0;
const MIN_FPS: f64 = 1.0;
const MAX_FPS: f64 = 240.0;

/// Parses an ffprobe frame-rate field into fps. The field is a rational like
/// `"30000/1001"` or `"30/1"`, occasionally a bare number, and `"0/0"` when the
/// stream doesn't report one. Returns `None` for anything unparseable or zero.
pub fn parse_frame_rate(text: &str) -> Option<f64> {
    let text = text.trim();
    if let Some((num, den)) = text.split_once('/') {
        let num: f64 = num.trim().parse().ok()?;
        let den: f64 = den.trim().parse().ok()?;
        if num == 0.0 || den == 0.0 {
            return None;
        }
        Some(num / den)
    } else {
        let fps: f64 = text.parse().ok()?;
        (fps != 0.0).then_some(fps)
    }
}

/// Snaps a dimension down to the nearest even number (minimum 2), as required by
/// H.264 yuv420p, which mandates even width and height.
pub fn make_even(dim: u32) -> u32 {
    (dim & !1).max(2)
}

/// Reads the average frame rate of `source` via `ffprobe`, falling back to 30
/// fps (with a warning) if it can't be determined. The new usls `DataLoader`
/// no longer exposes the source frame rate, so we probe it here. The result is
/// clamped to a sane range, since fps drives both output timing and smoothing.
pub fn probe_fps(source: &str) -> f64 {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "0",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=avg_frame_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(source)
        .output();

    let output = match output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            eprintln!(
                "warning: ffprobe exited with {} for {source:?}; defaulting to {DEFAULT_FPS} fps",
                output.status
            );
            return DEFAULT_FPS;
        }
        Err(err) => {
            eprintln!(
                "warning: could not run ffprobe ({err}) for {source:?}; defaulting to {DEFAULT_FPS} fps"
            );
            return DEFAULT_FPS;
        }
    };

    let text = String::from_utf8_lossy(&output.stdout);
    match parse_frame_rate(&text) {
        Some(fps) => fps.clamp(MIN_FPS, MAX_FPS),
        None => {
            eprintln!(
                "warning: could not parse frame rate from ffprobe output {:?} for {source:?}; defaulting to {DEFAULT_FPS} fps",
                text.trim()
            );
            DEFAULT_FPS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frame_rate_rational() {
        assert!((parse_frame_rate("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert!((parse_frame_rate("24000/1001").unwrap() - 23.976).abs() < 0.01);
        assert_eq!(parse_frame_rate("25/1"), Some(25.0));
        assert_eq!(parse_frame_rate("30/1"), Some(30.0));
    }

    #[test]
    fn test_parse_frame_rate_bare_and_whitespace() {
        assert_eq!(parse_frame_rate("25"), Some(25.0));
        assert_eq!(parse_frame_rate("  60/1\n"), Some(60.0));
    }

    #[test]
    fn test_parse_frame_rate_invalid() {
        assert_eq!(parse_frame_rate("0/0"), None);
        assert_eq!(parse_frame_rate("30/0"), None);
        assert_eq!(parse_frame_rate("0/1"), None);
        assert_eq!(parse_frame_rate(""), None);
        assert_eq!(parse_frame_rate("garbage"), None);
        assert_eq!(parse_frame_rate("abc/def"), None);
    }

    #[test]
    fn test_make_even() {
        assert_eq!(make_even(1080), 1080);
        assert_eq!(make_even(1081), 1080);
        assert_eq!(make_even(3), 2);
        assert_eq!(make_even(2), 2);
        assert_eq!(make_even(1), 2);
        assert_eq!(make_even(0), 2);
    }
}
