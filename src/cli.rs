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
    #[argh(option, default = "0.7")]
    pub object_prob_threshold: f32,

    /// object area threshold (minimum area as percentage of frame, ignored for ball objects)
    #[argh(option, default = "0.0025")]
    pub object_area_threshold: f32,

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

    /// keep graphic
    #[argh(switch)]
    pub keep_graphic: bool,

    /// prioritize graphic: check against graphic threshold regardless of object count
    #[argh(switch)]
    pub prioritize_graphic: bool,

    /// graphic threshold: percentage of frame area covered by detected HBBs (default: 0.01)
    #[argh(option, default = "0.009")]
    pub graphic_threshold: f32,

    /// add captions: extract audio, transcribe, burn captions, and recombine
    #[argh(switch)]
    pub add_captions: bool,

    /// output filepath: if set, move the final video to this location
    #[argh(option, default = "String::from(\"\")")]
    pub output_filepath: String,
}
