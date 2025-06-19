use anyhow::Result;
use usls::{Config, NAMES_COCO_80, NAMES_COCO_KEYPOINTS_17, NAMES_IMAGENET_1K};
use crate::cli::Args;

/// Builds a YOLO model configuration from command line arguments
pub fn build_config(args: &Args) -> Result<Config> {
    let mut config = Config::yolo()
        .with_model_file(args.model.as_ref().map_or("", String::as_str))
        .with_task(args.task.parse()?)
        .with_version(args.ver.try_into()?)
        .with_scale(args.scale.parse()?)
        .with_model_dtype(args.dtype.parse()?)
        .with_model_device(args.device.parse()?)
        .with_model_tensorrt_fp16(args.trt_fp16)
        .with_model_ixx(
            0,
            0,
            (args.min_batch_size, args.batch_size, args.max_batch_size).into(),
        )
        .with_model_ixx(
            0,
            2,
            (
                args.min_image_height,
                args.image_height,
                args.max_image_height,
            )
                .into(),
        )
        .with_model_ixx(
            0,
            3,
            (args.min_image_width, args.image_width, args.max_image_width).into(),
        )
        .with_class_confs(if args.confs.is_empty() {
            &[0.2, 0.15]
        } else {
            &args.confs
        })
        .with_keypoint_confs(if args.keypoint_confs.is_empty() {
            &[0.5]
        } else {
            &args.keypoint_confs
        })
        .with_topk(args.topk)
        .retain_classes(&args.retain_classes)
        .exclude_classes(&args.exclude_classes)
        .with_model_num_dry_run(2);

    // Apply class configurations
    if args.use_coco_80_classes {
        config = config.with_class_names(&NAMES_COCO_80);
    }
    if args.use_coco_17_keypoints_classes {
        config = config.with_keypoint_names(&NAMES_COCO_KEYPOINTS_17);
    }
    if args.use_imagenet_1k_classes {
        config = config.with_class_names(&NAMES_IMAGENET_1K);
    }
    if let Some(nc) = args.num_classes {
        config = config.with_nc(nc);
    }
    if let Some(nk) = args.num_keypoints {
        config = config.with_nk(nk);
    }
    if !args.class_names.is_empty() {
        config = config.with_class_names(
            &args
                .class_names
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>(),
        );
    }
    if !args.keypoint_names.is_empty() {
        config = config.with_keypoint_names(
            &args
                .keypoint_names
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>(),
        );
    }

    Ok(config)
} 