use anyhow::Result;
use usls::{Config, Task, NAMES_COCO_80};
use crate::cli::Args;

/// Determines the model file path based on object type, version, and scale
fn get_model_path(object: &str, ver: f32, scale: &str) -> String {
    match object {
        "faces" => {
            // Check if version and scale are supported for faces
            let supported_versions = [6.0, 8.0, 10.0, 11.0];
            let supported_scales = ["n", "s", "m", "l"];
            
            if supported_versions.contains(&ver) && supported_scales.contains(&scale) {
                format!("./model/yolov{}{}-face.onnx", ver as i32, scale)
            } else {
                // Default to yolov8m-face.onnx if unsupported combination
                "./model/yolov8m-face.onnx".to_string()
            }
        }
        "heads" => "./model/v8-head-fp16.onnx".to_string(),
        "football" => {
            match scale {
                "m" => "./model/yolov8m-football.onnx".to_string(),
                "n" => "./model/yolov8n-football.onnx".to_string(),
                _ => "./model/yolov8n-football.onnx".to_string(), // Default to n scale
            }
        }
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
        assert_eq!(get_model_path("faces", 8.0, "m"), "./model/yolov8m-face.onnx");
        assert_eq!(get_model_path("faces", 10.0, "s"), "./model/yolov10s-face.onnx");
        assert_eq!(get_model_path("faces", 11.0, "l"), "./model/yolov11l-face.onnx");
        assert_eq!(get_model_path("faces", 6.0, "n"), "./model/yolov6n-face.onnx");
        
        // Test unsupported combination defaults to yolov8m-face.onnx
        assert_eq!(get_model_path("faces", 9.0, "m"), "./model/yolov8m-face.onnx");
        assert_eq!(get_model_path("faces", 8.0, "x"), "./model/yolov8m-face.onnx");
        
        // Test heads
        assert_eq!(get_model_path("heads", 8.0, "m"), "./model/v8-head-fp16.onnx");
        
        // Test football
        assert_eq!(get_model_path("football", 8.0, "m"), "./model/yolov8m-football.onnx");
        assert_eq!(get_model_path("football", 8.0, "n"), "./model/yolov8n-football.onnx");
        assert_eq!(get_model_path("football", 8.0, "s"), "./model/yolov8n-football.onnx"); // Defaults to n
        
        // Test other object types
        assert_eq!(get_model_path("person", 8.0, "m"), "");
        assert_eq!(get_model_path("car", 8.0, "m"), "");
        assert_eq!(get_model_path("sports ball", 8.0, "m"), "");
    }
} 