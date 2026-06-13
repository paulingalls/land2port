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

    /// Displays (unless headless) and encodes one output frame.
    ///
    /// The encoder is created lazily from the first frame's dimensions, mirroring
    /// how the usls `Viewer` itself initializes encoding. Output frame timing is
    /// derived from a monotonic frame counter at the source fps, matching the old
    /// `Viewer::with_fps` behavior.
    pub fn write_frame(&mut self, img: &Image, headless: bool) -> Result<()> {
        if !headless {
            self.viewer.imshow(img)?;
        }

        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);

        if self.encoder.is_none() {
            let settings = Settings::preset_h264_yuv420p(w, h, false);
            self.encoder = Some(Encoder::new(self.saveout.clone(), settings)?);
        }

        let frame = Frame::from_shape_vec((h, w, 3), rgb.into_raw())?;
        let timestamp = Time::from_secs_f64(self.frame_index as f64 / self.fps);
        self.encoder
            .as_mut()
            .expect("encoder initialized above")
            .encode(&frame, timestamp)?;
        self.frame_index += 1;
        Ok(())
    }

    /// Flushes and closes the encoder, finalizing the output file.
    pub fn finalize(&mut self) -> Result<()> {
        if let Some(mut encoder) = self.encoder.take() {
            encoder.finish()?;
        }
        Ok(())
    }
}

/// Reads the average frame rate of `source` via `ffprobe`, falling back to 30.0
/// fps if it can't be determined (e.g. for non-file sources). The new usls
/// `DataLoader` no longer exposes the source frame rate, so we probe it here.
pub fn probe_fps(source: &str) -> f64 {
    const DEFAULT_FPS: f64 = 30.0;

    let output = Command::new("ffprobe")
        .args([
            "-v",
            "0",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=r_frame_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(source)
        .output();

    let Ok(output) = output else {
        return DEFAULT_FPS;
    };

    // r_frame_rate is reported as a rational, e.g. "30000/1001" or "30/1".
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if let Some((num, den)) = text.split_once('/') {
        if let (Ok(num), Ok(den)) = (num.parse::<f64>(), den.parse::<f64>()) {
            if den != 0.0 && num != 0.0 {
                return num / den;
            }
        }
    } else if let Ok(fps) = text.parse::<f64>() {
        if fps != 0.0 {
            return fps;
        }
    }

    DEFAULT_FPS
}
