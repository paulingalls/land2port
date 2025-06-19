use argh::FromArgs;

/// YOLO Example
#[derive(FromArgs, Debug)]
pub struct Args {
    /// model file(.onnx)
    #[argh(option)]
    pub model: Option<String>,

    /// source: image, image folder, video stream
    #[argh(option, default = "String::from(\"./video/video1.mp4\")")]
    pub source: String,

    /// model dtype
    #[argh(option, default = "String::from(\"auto\")")]
    pub dtype: String,

    /// task: det, seg, pose, classify, obb
    #[argh(option, default = "String::from(\"det\")")]
    pub task: String,

    /// version
    #[argh(option, default = "8.0")]
    pub ver: f32,

    /// device: cuda, cpu, mps
    #[argh(option, default = "String::from(\"cpu:0\")")]
    pub device: String,

    /// scale: n, s, m, l, x
    #[argh(option, default = "String::from(\"m\")")]
    pub scale: String,

    /// enable TensorRT FP16
    #[argh(option, default = "true")]
    pub trt_fp16: bool,

    /// batch size
    #[argh(option, default = "1")]
    pub batch_size: usize,

    /// bin batch size: For TensorRT
    #[argh(option, default = "1")]
    pub min_batch_size: usize,

    /// max Batch size: For TensorRT
    #[argh(option, default = "4")]
    pub max_batch_size: usize,

    /// min image width: For TensorRT
    #[argh(option, default = "224")]
    pub min_image_width: isize,

    /// image width: For TensorRT
    #[argh(option, default = "640")]
    pub image_width: isize,

    /// max image width: For TensorRT
    #[argh(option, default = "1920")]
    pub max_image_width: isize,

    /// min image height: For TensorRT
    #[argh(option, default = "224")]
    pub min_image_height: isize,

    /// image height: For TensorRT
    #[argh(option, default = "640")]
    pub image_height: isize,

    /// max image height: For TensorRT
    #[argh(option, default = "1920")]
    pub max_image_height: isize,

    /// num classes
    #[argh(option)]
    pub num_classes: Option<usize>,

    /// num keypoints
    #[argh(option)]
    pub num_keypoints: Option<usize>,

    /// class names
    #[argh(option)]
    pub class_names: Vec<String>,

    /// keypoint names
    #[argh(option)]
    pub keypoint_names: Vec<String>,

    /// top-k
    #[argh(option, default = "5")]
    pub topk: usize,

    /// use COCO 80 classes
    #[argh(switch)]
    pub use_coco_80_classes: bool,

    /// use COCO 17 keypoints classes
    #[argh(switch)]
    pub use_coco_17_keypoints_classes: bool,

    /// use ImageNet 1K classes
    #[argh(switch)]
    pub use_imagenet_1k_classes: bool,

    /// confidences
    #[argh(option)]
    pub confs: Vec<f32>,

    /// keypoint nonfidences
    #[argh(option)]
    pub keypoint_confs: Vec<f32>,

    /// exclude nlasses
    #[argh(option)]
    pub exclude_classes: Vec<usize>,

    /// retain classes
    #[argh(option)]
    pub retain_classes: Vec<usize>,

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
