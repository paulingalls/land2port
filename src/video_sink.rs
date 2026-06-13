use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use usls::{Image, Key, Viewer};
use video_rs::{encode::Settings, Encoder, Frame, Time};

/// Sink for processed frames.
///
/// Wraps a usls [`Viewer`] (for the optional preview window) and a video-rs
/// [`Encoder`] (for writing the cropped output to a deterministic path). The
/// usls `Viewer` auto-generates output paths and exposes no save-path API, so
/// we drive the encoder ourselves to keep writing to the path `main.rs` expects.
pub struct VideoSink {
    viewer: Viewer<'static>,
    encoder: Option<Encoder>,
    saveout: PathBuf,
    fps: f64,
    frame_index: usize,
}

impl VideoSink {
    /// Creates a sink that encodes to `saveout` at the given frames-per-second.
    pub fn new(saveout: impl Into<PathBuf>, fps: f64) -> Self {
        Self {
            viewer: Viewer::default().with_window_scale(0.5),
            encoder: None,
            saveout: saveout.into(),
            fps,
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

    /// Displays (unless headless) and encodes one output frame, consuming it.
    ///
    /// Takes the cropped image by value so the pixel buffer can be moved into the
    /// encoder (`into_rgb8`) rather than cloned. The encoder is created lazily
    /// from the first frame's dimensions, mirroring how the usls `Viewer` itself
    /// initializes encoding. Output frame timing is derived from a monotonic
    /// frame counter at the source fps, matching the old `Viewer::with_fps`.
    pub fn write_frame(&mut self, img: Image, headless: bool) -> Result<()> {
        if !headless {
            self.viewer.imshow(&img)?;
        }

        let rgb = img.into_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);

        if self.encoder.is_none() {
            let settings = Settings::preset_h264_yuv420p(w, h, false);
            self.encoder = Some(Encoder::new(self.saveout.clone(), settings)?);
        }
        let encoder = self.encoder.as_mut().expect("encoder initialized above");

        let frame = Frame::from_shape_vec((h, w, 3), rgb.into_raw())?;
        let timestamp = Time::from_secs_f64(self.frame_index as f64 / self.fps);
        encoder.encode(&frame, timestamp)?;
        self.frame_index += 1;
        Ok(())
    }

    /// Number of frames encoded so far.
    pub fn frame_count(&self) -> usize {
        self.frame_index
    }

    /// Flushes and closes the encoder, finalizing the output file.
    pub fn finalize(&mut self) -> Result<()> {
        if let Some(mut encoder) = self.encoder.take() {
            encoder.finish()?;
        }
        Ok(())
    }
}

impl Drop for VideoSink {
    fn drop(&mut self) {
        // Ensure the mp4 is finalized (moov atom written, packets flushed) even
        // if the processing loop exits early via an error or panic before the
        // explicit finalize() runs. A successful finalize() already took the
        // encoder, so this is a no-op on the happy path. Errors can't propagate
        // out of drop, so they are logged.
        if let Some(mut encoder) = self.encoder.take() {
            if let Err(err) = encoder.finish() {
                eprintln!("warning: failed to finalize video output on drop: {err}");
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
