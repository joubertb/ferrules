#![feature(portable_simd)]
use anyhow::Context;
use colored::*;
use render::{html::to_html, markdown::to_markdown};
use std::{
    fs::{create_dir, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use entities::ParsedDocument;

pub(crate) mod draw;
pub mod parse;

pub mod blocks;
pub mod entities;
pub mod layout;
pub mod render;

pub mod ocr;

const IMAGE_PADDING: u32 = 5;

fn sanitize_doc_name(doc_name: &str) -> String {
    doc_name
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c)
            } else if c.is_whitespace() {
                None
            } else {
                Some('-')
            }
        })
        .collect::<String>()
}

fn save_doc_images(imgs_dir: &Path, doc: &ParsedDocument) -> anyhow::Result<()> {
    for block in doc.blocks.iter() {
        match &block.kind {
            blocks::BlockType::Image(img_block) => {
                let page_id = block.pages_id.first().unwrap();
                match doc.pages.iter().find(|&p| p.id == *page_id) {
                    Some(page) => {
                        assert!(page.height as u32 > 0);
                        assert!(page.width as u32 > 0);

                        let x = (block.bbox.x0 - IMAGE_PADDING as f32) as u32;
                        let y = (block.bbox.y0 - IMAGE_PADDING as f32) as u32;
                        let width = (block.bbox.width().max(1.0) as u32 + 2 * IMAGE_PADDING)
                            .min(page.width as u32);
                        let height = (block.bbox.height().max(1.0) as u32 + 2 * IMAGE_PADDING)
                            .min(page.height as u32);

                        let crop = page.image.clone().crop(x, y, width, height);

                        let output_file = imgs_dir.join(img_block.path());
                        crop.save(output_file)?;
                    }
                    None => continue,
                }
            }
            blocks::BlockType::Table => todo!(),
            _ => continue,
        }
    }
    Ok(())
}

pub fn create_dirs<P: AsRef<Path>>(
    output_dir: Option<P>,
    doc_name: &str,
    debug: bool,
    save_imgs: bool,
) -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
    let result_dir_name = format!("{}-results", sanitize_doc_name(doc_name));
    let res_dir_path = match output_dir {
        Some(p) => p.as_ref().to_owned().join(&result_dir_name),
        None => {
            if std::fs::create_dir(&result_dir_name).is_err() {
                std::fs::remove_dir_all(&result_dir_name)?;
                std::fs::create_dir(&result_dir_name)?;
            };
            format!("./{}", &result_dir_name).into()
        }
    };
    if save_imgs {
        let debug_path = res_dir_path.join("figures");
        create_dir(&debug_path).context("cant create debug path")?;
    }

    let debug_path = if debug {
        let debug_path = res_dir_path.join("debug");
        create_dir(&debug_path).context("cant create debug path")?;
        Some(debug_path)
    } else {
        None
    };
    Ok((res_dir_path, debug_path))
}

pub fn save_parsed_document(
    doc: &ParsedDocument,
    res_dir_path: PathBuf,
    save_imgs: bool,
    save_html: bool,
    save_markdown: bool,
) -> anyhow::Result<()> {
    let sanitized_doc_name = sanitize_doc_name(&doc.doc_name);
    // Save json
    let file_out = res_dir_path.join(format!("{}.json", &sanitized_doc_name));
    let file = File::create(&file_out)?;
    let mut writer = BufWriter::new(file);
    let doc_json = serde_json::to_string(&doc)?;
    writer.write_all(doc_json.as_bytes())?;
    // TODO: this is shit, refac
    let fig_path = PathBuf::from_str("figures").unwrap();

    if save_imgs {
        save_doc_images(&res_dir_path.join(&fig_path), doc).context("can't save the doc images")?;
    }

    if let Some(dbg_path) = &doc.debug_path {
        println!(
            "{} Debug output saved in: {}",
            "ℹ".yellow().bold(),
            dbg_path.display().to_string().yellow().underline()
        );
    }

    if save_html {
        if !save_imgs {
            save_doc_images(&res_dir_path.join(&fig_path), doc)
                .context("can't save the doc images")?;
        }
        let html_content = to_html(doc, &doc.doc_name, Some(fig_path.clone())).unwrap();
        let html_file_out = res_dir_path.join(format!("{}.html", sanitized_doc_name));
        let file = File::create(&html_file_out)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(html_content.as_bytes())?;
    }

    if save_markdown {
        let md_content = to_markdown(doc, &doc.doc_name, Some(fig_path.clone())).unwrap();
        let html_file_out = res_dir_path.join(format!("{}.md", sanitized_doc_name));
        let file = File::create(&html_file_out)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(md_content.as_bytes())?;
    }
    println!(
        "{} Results saved in: {}",
        "✓".green().bold(),
        res_dir_path.display().to_string().cyan().underline()
    );

    Ok(())
}
