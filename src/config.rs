use crate::cli::Args;
use anyhow::Result;
use usls::{Config, NAMES_COCO_80, Task};

/// Determines the model reference for a given object type, version, and scale.
///
/// These are not local paths but references that usls's `Hub` resolves and
/// caches on first use (the usls cache dir, e.g. `~/.cache/usls` on Linux or
/// `~/Library/Caches/usls` on macOS), so the weights don't have to be
/// vendored into this (MIT-licensed) repository. A local file at the same path
/// still takes precedence if present, so an offline `./model/…` copy keeps working.
///
/// Sources (all weights remain under their upstream AGPL/GPL licenses — see NOTICE):
/// - **face** → Hugging Face `deepghs/yolo-face` (ONNX exports of akanametov/yolo-face)
/// - **head** → `jamjamjon/assets` GitHub release (usls's default hub, tag `yolo`)
fn get_model_path(object: &str, ver: f32, scale: &str) -> String {
    match object {
        "face" => {
            // Check if version and scale are supported for faces
            let supported_versions = [6.0, 8.0, 10.0, 11.0];
            let supported_scales = ["n", "s", "m", "l"];

            if supported_versions.contains(&ver) && supported_scales.contains(&scale) {
                format!("deepghs/yolo-face/yolov{}{}-face/model.onnx", ver as i32, scale)
            } else {
                // Default to yolov8m-face if unsupported combination
                "deepghs/yolo-face/yolov8m-face/model.onnx".to_string()
            }
        }
        "head" => "yolo/v8-head-fp16.onnx".to_string(),
        _ => "".to_string(), // Empty string for other object types
    }
}

/// Builds a YOLO model configuration from command line arguments
pub fn build_config(args: &Args) -> Result<Config> {
    let model_path = get_model_path(&args.object, args.ver, &args.scale);

    let mut config = Config::yolo()
        .with_task(Task::ObjectDetection)
        .with_model_file(&model_path)
        .with_version(args.ver.try_into()?)
        .with_scale(args.scale.parse()?)
        .with_model_dtype(args.dtype.parse()?)
        .with_model_device(args.device.parse()?)
        .with_model_num_dry_run(2);

    if model_path.is_empty() {
        config = config.with_class_names(&NAMES_COCO_80);
        config = match args.object.as_str() {
            "person" => config.retain_classes(&[0]),
            "car" => config.retain_classes(&[2]),
            "motorcycle" => config.retain_classes(&[3]),
            "truck" => config.retain_classes(&[7]),
            "boat" => config.retain_classes(&[8]),
            "frisbee" => config.retain_classes(&[29]),
            "sports ball" => config.retain_classes(&[32]),
            _ => config,
        };
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_model_path() {
        // Test faces with different versions and scales
        assert_eq!(
            get_model_path("face", 8.0, "m"),
            "deepghs/yolo-face/yolov8m-face/model.onnx"
        );
        assert_eq!(
            get_model_path("face", 10.0, "s"),
            "deepghs/yolo-face/yolov10s-face/model.onnx"
        );
        assert_eq!(
            get_model_path("face", 11.0, "l"),
            "deepghs/yolo-face/yolov11l-face/model.onnx"
        );
        assert_eq!(
            get_model_path("face", 6.0, "n"),
            "deepghs/yolo-face/yolov6n-face/model.onnx"
        );

        // Test unsupported combination defaults to yolov8m-face
        assert_eq!(
            get_model_path("face", 9.0, "m"),
            "deepghs/yolo-face/yolov8m-face/model.onnx"
        );
        assert_eq!(
            get_model_path("face", 8.0, "x"),
            "deepghs/yolo-face/yolov8m-face/model.onnx"
        );

        // Test heads (usls default hub: jamjamjon/assets, tag `yolo`)
        assert_eq!(get_model_path("head", 8.0, "m"), "yolo/v8-head-fp16.onnx");

        // Test other object types (COCO passthrough / no dedicated model)
        assert_eq!(get_model_path("person", 8.0, "m"), "");
        assert_eq!(get_model_path("car", 8.0, "m"), "");
        assert_eq!(get_model_path("sports ball", 8.0, "m"), "");
    }
}
