use argh::FromArgs;

/// YOLO Example
#[derive(FromArgs, Debug)]
pub struct Args {
    /// object type: faces, heads, football, sports ball, frisbee, person, car, truck, or boat
    #[argh(option, default = "String::from(\"faces\")")]
    pub object: String,

    /// source: image, image folder, video stream
    #[argh(option, default = "String::from(\"./video/video1.mp4\")")]
    pub source: String,

    /// model dtype
    #[argh(option, default = "String::from(\"auto\")")]
    pub dtype: String,

    /// version
    #[argh(option, default = "8.0")]
    pub ver: f32,

    /// device: cuda, cpu, mps
    #[argh(option, default = "String::from(\"cpu:0\")")]
    pub device: String,

    /// scale: n, s, m, l
    #[argh(option, default = "String::from(\"m\")")]
    pub scale: String,

    /// smooth percentage threshold
    #[argh(option, default = "10.0")]
    pub smooth_percentage: f32,

    /// smooth duration in frames
    #[argh(option, default = "45")]
    pub smooth_duration: usize,

    /// use headless mode
    #[argh(switch)]
    pub headless: bool,

    /// enable stack crop
    #[argh(switch)]
    pub use_stack_crop: bool,
}
