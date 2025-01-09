use image::{DynamicImage, ImageBuffer, Rgba};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect;

use crate::Line;

use super::model::LayoutBBox;

use ab_glyph::FontArc;
pub fn load_font() -> FontArc {
    use std::path::Path;
    let font_path = Path::new("./font/Arial.ttf");
    let buffer = std::fs::read(font_path).unwrap();
    FontArc::try_from_vec(buffer).unwrap()
}

pub(crate) fn draw_text_lines(
    lines: &[Line],
    page_img: &DynamicImage,
    rescale_factor: f32,
) -> anyhow::Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // Convert the dynamic image to RGBA for in-place drawing.
    let mut out_img = page_img.to_rgba8();

    let font: FontArc = load_font();
    // Iterate over all bounding boxes and draw them.
    for line in lines {
        let x0 = (rescale_factor * line.bbox.x0) as i32;
        let y0 = (rescale_factor * line.bbox.y0) as i32;
        let x1 = (rescale_factor * line.bbox.x1) as i32;
        let y1 = (rescale_factor * line.bbox.y1) as i32;

        // Determine rectangle width/height based on (x0, y0), (x1, y1).
        // Assuming x1 >= x0, y1 >= y0 in your data:
        let width = (x1 - x0).max(1) as u32;
        let height = (y1 - y0).max(1) as u32;

        // Create a Rect with top-left at (x0, y0) and size (width, height).
        let rect = Rect::at(x0, y0).of_size(width, height);

        // Draw a hollow rectangle (box outline only).
        // Use RGBA color [R, G, B, A] (e.g., red with full opacity).
        draw_hollow_rect_mut(&mut out_img, rect, Rgba([255, 0, 0, 255]));
        let scale = 80;
        let legend_size = page_img.width().max(page_img.height()) / scale;
        imageproc::drawing::draw_text_mut(
            &mut out_img,
            image::Rgba([254u8, 0u8, 0u8, 0u8]),
            line.bbox.x0 as i32,
            (line.bbox.y0 - legend_size as f32) as i32,
            legend_size as f32,
            &font,
            "line",
        );
    }

    Ok(out_img)
}

pub(crate) fn draw_bboxes(
    bboxes: &[LayoutBBox],
    page_img: &DynamicImage,
) -> anyhow::Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // Convert the dynamic image to RGBA for in-place drawing.
    let mut out_img = page_img.to_rgba8();

    let font: FontArc = load_font();
    // Iterate over all bounding boxes and draw them.
    for layout_box in bboxes {
        let x0 = layout_box.bbox.x0 as i32;
        let y0 = layout_box.bbox.y0 as i32;
        let x1 = layout_box.bbox.x1 as i32;
        let y1 = layout_box.bbox.y1 as i32;

        // Determine rectangle width/height based on (x0, y0), (x1, y1).
        // Assuming x1 >= x0, y1 >= y0 in your data:
        let width = (x1 - x0).max(0) as u32;
        let height = (y1 - y0).max(0) as u32;

        // Create a Rect with top-left at (x0, y0) and size (width, height).
        let rect = Rect::at(x0, y0).of_size(width, height);

        // Draw a hollow rectangle (box outline only).
        // Use RGBA color [R, G, B, A] (e.g., red with full opacity).
        draw_hollow_rect_mut(&mut out_img, rect, Rgba([0u8, 0u8, 255u8, 255u8]));
        let legend = format!("{} {:.2}", layout_box.label, layout_box.proba);
        let scale = 50;
        let legend_size = page_img.width().max(page_img.height()) / scale;
        imageproc::drawing::draw_text_mut(
            &mut out_img,
            image::Rgba([0u8, 0u8, 255u8, 255u8]),
            layout_box.bbox.x0 as i32,
            (layout_box.bbox.y0 - legend_size as f32) as i32,
            legend_size as f32,
            &font,
            &legend,
        );
    }

    Ok(out_img)
}
