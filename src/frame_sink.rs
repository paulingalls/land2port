use crate::metrics;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::Instant;
use usls::{Image, Key, Viewer};
use video_rs::encode::{Encoder, Settings};
use video_rs::frame::Frame;
use video_rs::time::Time;

/// One encode request: the cropped frame's RGB24 bytes plus its dimensions.
struct EncodeMsg {
    w: usize,
    h: usize,
    data: Vec<u8>,
}

/// Frame sink that decouples H.264 encoding from the processing loop.
///
/// The usls `Viewer` is retained only for on-screen display (non-headless mode)
/// and key handling. Encoding runs on a dedicated thread fed by a bounded
/// channel, so the (~55 s on L4) encode overlaps crop rendering and detection
/// instead of running serially after them.
///
/// Output is byte-identical to the previous inline path: the channel is FIFO
/// and single-consumer, so frames are encoded in exactly the order they were
/// produced, with the same per-frame timestamps the Viewer used.
///
/// The `video-rs` `Encoder` wraps non-`Send` ffmpeg state, so it is constructed
/// and owned entirely inside the encoder thread — only plain frame bytes
/// (which are `Send`) cross the channel.
pub struct FrameSink {
    viewer: Viewer<'static>,
    headless: bool,
    tx: Option<SyncSender<EncodeMsg>>,
    handle: Option<JoinHandle<Result<()>>>,
}

impl FrameSink {
    /// Creates a sink that writes H.264 to `saveout` at `fps`. The `viewer` is
    /// used only for display/keys; pass `headless = true` to skip display.
    pub fn new(viewer: Viewer<'static>, saveout: String, fps: f32, headless: bool) -> Self {
        // Bounded so a slow encoder applies backpressure rather than letting
        // in-flight frames (each ~6 MB at 1080x1920) grow unbounded in RAM.
        let (tx, rx) = sync_channel::<EncodeMsg>(8);

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut encoder: Option<Encoder> = None;
            let mut position = Time::zero();
            let step = Time::from_secs(1.0 / fps);

            while let Ok(msg) = rx.recv() {
                let start = Instant::now();
                if encoder.is_none() {
                    let settings = Settings::preset_h264_yuv420p(msg.w, msg.h, false);
                    encoder = Some(
                        Encoder::new(PathBuf::from(&saveout), settings)
                            .context("creating video encoder")?,
                    );
                }
                let enc = encoder.as_mut().unwrap();
                let frame = Frame::from_shape_vec((msg.h, msg.w, 3), msg.data)
                    .context("building encoder frame")?;
                enc.encode(&frame, position).context("encoding video frame")?;
                position = position.aligned_with(step).add();
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
            viewer,
            headless,
            tx: Some(tx),
            handle: Some(handle),
        }
    }

    /// Displays (non-headless) and enqueues a finished cropped frame for
    /// encoding. Blocks if the encoder is more than the channel bound behind.
    pub fn write_frame(&mut self, cropped: &Image) -> Result<()> {
        if !self.headless {
            self.viewer.imshow(cropped)?;
        }
        let (w, h) = cropped.dimensions();
        // Clone the RGB24 bytes to hand ownership to the encoder thread (the
        // previous inline path cloned here too, so this is not extra cost).
        let data = cropped.as_raw().clone();
        self.tx
            .as_ref()
            .expect("write_frame after finish")
            .send(EncodeMsg {
                w: w as usize,
                h: h as usize,
                data,
            })
            .map_err(|_| anyhow::anyhow!("encoder thread terminated early"))?;
        Ok(())
    }

    /// Flushes and finalizes: drops the sender so the encoder thread drains its
    /// queue and writes the container trailer, then joins and propagates any
    /// encode error.
    pub fn finish(&mut self) -> Result<()> {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("encoder thread panicked"))??;
        }
        Ok(())
    }

    // --- display / window delegation to the inner viewer ---

    pub fn wait_key(&mut self, delay_ms: u64) -> Option<Key> {
        self.viewer.wait_key(delay_ms)
    }

    pub fn is_window_open(&self) -> bool {
        self.viewer.is_window_open()
    }

    pub fn is_window_exist(&self) -> bool {
        self.viewer.is_window_exist()
    }
}
