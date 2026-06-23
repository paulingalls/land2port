use argh::FromArgs;

/// YOLO Example
#[derive(FromArgs, Debug)]
pub struct Args {
    /// object type: face, head, ball, sports ball, frisbee, person, car, truck, or boat
    #[argh(option, default = "String::from(\"face\")")]
    pub object: String,

    /// source: image, image folder, video stream
    #[argh(option, default = "String::from(\"./video/video1.mp4\")")]
    pub source: String,

    /// model dtype
    #[argh(option, default = "String::from(\"auto\")")]
    pub dtype: String,

    /// version
    #[argh(option, default = "11.0")]
    pub ver: f32,

    /// device: cuda, cpu, coreml
    #[argh(option, default = "String::from(\"cpu:0\")")]
    pub device: String,

    /// scale: n, s, m, l
    #[argh(option, default = "String::from(\"s\")")]
    pub scale: String,

    /// smooth percentage threshold
    #[argh(option, default = "7.5")]
    pub smooth_percentage: f32,

    /// smooth duration in seconds
    #[argh(option, default = "1.0")]
    pub smooth_duration: f32,

    /// object probability threshold
    #[argh(option, default = "0.75")]
    pub object_prob_threshold: f32,

    /// minimum object area as a fraction of the LARGEST detected object's area;
    /// smaller detections (e.g. faces printed on a book cover/poster, or distant
    /// bystanders) are dropped so they don't inflate the head count and trigger a
    /// stacked layout that splits the real subject. Default 0.1 (an object must be
    /// at least ~1/3 the dominant object's height to count). 0 disables; ball is exempt.
    #[argh(option, default = "0.1")]
    pub min_area_ratio: f32,

    /// cut similarity threshold (default: 0.4)
    #[argh(option, default = "0.4")]
    pub cut_similarity: f64,

    /// cut start threshold (default: 0.8)
    #[argh(option, default = "0.8")]
    pub cut_start: f64,

    /// use headless mode
    #[argh(switch)]
    pub headless: bool,

    /// enable stack crop
    #[argh(switch)]
    pub use_stack_crop: bool,

    /// use simple smoothing instead of history smoothing
    #[argh(switch)]
    pub use_simple_smoothing: bool,

    /// keep text
    #[argh(switch)]
    pub keep_text: bool,

    /// prioritize text: check against text threshold regardless of object count
    #[argh(switch)]
    pub prioritize_text: bool,

    /// text threshold: percentage of frame area covered by detected text (default: 0.01)
    #[argh(option, default = "0.008")]
    pub text_area_threshold: f32,

    /// text probability threshold: minimum confidence for text detections (default: 0.85)
    #[argh(option, default = "0.85")]
    pub text_prob_threshold: f32,

    /// add captions: extract audio, transcribe, burn captions, and recombine
    #[argh(switch)]
    pub add_captions: bool,

    /// output filepath: if set, move the final video to this location
    #[argh(option, default = "String::from(\"\")")]
    pub output_filepath: String,

    /// local-stage: copy the source to local disk before processing and write
    /// the output locally before copying to output-filepath, avoiding decode/
    /// encode directly over a network mount (e.g. GCS FUSE on Cloud Run)
    #[argh(switch)]
    pub local_stage: bool,
}
