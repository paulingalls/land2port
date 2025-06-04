use anyhow::Result;
use usls::{
    Annotator, DataLoader, SKELETON_COCO_19, SKELETON_COLOR_COCO_19, Style, Viewer, models::YOLO,
};

mod cli;
mod config;
mod crop;
mod image;

fn main() -> Result<()> {
    let args: cli::Args = argh::from_env();
    let config = config::build_config(&args)?;

    let mut viewer = Viewer::default().with_window_scale(0.5).with_fps(30);

    // build model
    let mut model = YOLO::new(config.commit()?)?;

    // build dataloader
    let dl = DataLoader::new(&args.source)?
        .with_batch(model.batch() as _)
        .build()?;

    // build annotator
    let annotator = Annotator::default()
        .with_obb_style(Style::obb().with_draw_fill(true))
        .with_hbb_style(
            Style::hbb()
                .with_draw_fill(true)
                .with_palette(&usls::Color::palette_coco_80()),
        )
        .with_keypoint_style(
            Style::keypoint()
                .with_skeleton((SKELETON_COCO_19, SKELETON_COLOR_COCO_19).into())
                .show_confidence(false)
                .show_id(true)
                .show_name(false),
        )
        .with_mask_style(Style::mask().with_draw_mask_polygon_largest(true));

    // Store the previous crop result
    let mut previous_crop: Option<crop::CropResult> = None;

    // run & annotate
    for xs in &dl {
        if viewer.is_window_exist() && !viewer.is_window_open() {
            break;
        }

        // Handle key events and delay
        if let Some(key) = viewer.wait_key(1) {
            if key == usls::Key::Escape {
                break;
            }
        }

        let ys = model.forward(&xs)?;
        println!("ys: {:?}", ys);

        for (x, y) in xs.iter().zip(ys.iter()) {
            let img = annotator.annotate(x, y)?;

            // Calculate crop areas based on the detection results
            let new_crop = crop::calculate_crop_area(
                img.width() as f32,
                img.height() as f32,
                y,
                0.5, // head probability threshold
            )?;

            // Compare with previous crop if it exists
            let crop_result = if let Some(prev_crop) = &previous_crop {
                let should_use_prev = match (&new_crop, prev_crop) {
                    (crop::CropResult::Single(new), crop::CropResult::Single(prev)) => {
                        new.is_within_percentage(prev, 10.0)
                    }
                    (crop::CropResult::Stacked(new1, new2), crop::CropResult::Stacked(prev1, prev2)) => {
                        new1.is_within_percentage(prev1, 10.0) && new2.is_within_percentage(prev2, 10.0)
                    }
                    _ => false, // If crop types don't match, use the new crop
                };

                if should_use_prev {
                    prev_crop.clone()
                } else {
                    new_crop.clone()
                }
            } else {
                new_crop.clone()
            };

            previous_crop = Some(crop_result.clone());

            println!("crop_result: {:?}", crop_result);

            // Create the cropped image
            let cropped_img = image::create_cropped_image(&img, &crop_result, img.height() as u32)?;

            // Display the cropped image
            viewer.imshow(&cropped_img)?;
            viewer.write_video_frame(&cropped_img)?;
        }
    }

    viewer.finalize_video()?;

    // summary
    model.summary();

    Ok(())
}
