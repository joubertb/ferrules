use image::{DynamicImage, ImageBuffer, Rgba};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect;

use crate::entities::Line;
use crate::ocr::OCRLines;

use super::model::LayoutBBox;

use ab_glyph::FontArc;

static FONT_BYTES: &[u8] = include_bytes!("../../font/Arial.ttf");

pub fn load_font() -> FontArc {
    FontArc::try_from_slice(FONT_BYTES).unwrap()
}

pub(crate) fn draw_text_lines(
    lines: &[Line],
    page_img: &DynamicImage,
) -> anyhow::Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // Convert the dynamic image to RGBA for in-place drawing.
    let mut out_img = page_img.to_rgba8();

    let font: FontArc = load_font();
    // Iterate over all bounding boxes and draw them.
    for line in lines {
        let x0 = (line.bbox.x0) as i32;
        let y0 = (line.bbox.y0) as i32;
        let x1 = (line.bbox.x1) as i32;
        let y1 = (line.bbox.y1) as i32;

        let width = (x1 - x0).max(1) as u32;
        let height = (y1 - y0).max(1) as u32;

        let rect = Rect::at(x0, y0).of_size(width, height);

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

pub(crate) fn draw_layout_bboxes(
    bboxes: &[LayoutBBox],
    page_img: &DynamicImage,
) -> anyhow::Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // Convert the dynamic image to RGBA for in-place drawing.
    let mut out_img = page_img.to_rgba8();

    let font: FontArc = load_font();
    for layout_box in bboxes {
        let x0 = layout_box.bbox.x0 as i32;
        let y0 = layout_box.bbox.y0 as i32;
        let x1 = layout_box.bbox.x1 as i32;
        let y1 = layout_box.bbox.y1 as i32;

        let width = (x1 - x0).max(0) as u32;
        let height = (y1 - y0).max(0) as u32;

        let rect = Rect::at(x0, y0).of_size(width, height);

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

pub(crate) fn draw_ocr_bboxes(
    bboxes: &[OCRLines],
    page_img: &DynamicImage,
) -> anyhow::Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    // Convert the dynamic image to RGBA for in-place drawing.
    let mut out_img = page_img.to_rgba8();

    let font: FontArc = load_font();
    for ocr_box in bboxes {
        let x0 = ocr_box.bbox.x0 as i32;
        let y0 = ocr_box.bbox.y0 as i32;
        let x1 = ocr_box.bbox.x1 as i32;
        let y1 = ocr_box.bbox.y1 as i32;

        let width = (x1 - x0).max(0) as u32;
        let height = (y1 - y0).max(0) as u32;

        let rect = Rect::at(x0, y0).of_size(width, height);

        draw_hollow_rect_mut(&mut out_img, rect, Rgba([17u8, 138u8, 1u8, 255u8]));
        let legend = format!("{} ({:.2})", ocr_box.text, ocr_box.confidence);
        let scale = 70;
        let legend_size = page_img.width().max(page_img.height()) / scale;
        imageproc::drawing::draw_text_mut(
            &mut out_img,
            image::Rgba([17u8, 138u8, 1u8, 255u8]),
            ocr_box.bbox.x0 as i32,
            (ocr_box.bbox.y0 - legend_size as f32) as i32,
            legend_size as f32,
            &font,
            &legend,
        );
    }

    Ok(out_img)
}
