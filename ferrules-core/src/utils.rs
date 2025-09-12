use crate::{
    blocks,
    entities::ParsedDocument,
    render::{html::to_html, markdown::to_markdown},
};

const IMAGE_PADDING: u32 = 5;
use anyhow::Context;
use colored::*;
use std::{
    fs::{create_dir, File},
    io::{BufWriter, Write},
    ops::Range,
    path::{Path, PathBuf},
    str::FromStr,
};

pub fn get_doc_length<P: AsRef<Path>>(
    path: P,
    _password: Option<&str>,
    page_range: Option<Range<usize>>,
) -> anyhow::Result<usize> {
    // Use lopdf instead of pdfium for simple page counting to avoid macOS hanging issue
    use lopdf::Document;

    let doc = Document::load(path).context("Failed to load PDF with lopdf")?;
    let page_count = doc.get_pages().len();

    match page_range {
        Some(range) => {
            if range.end > page_count {
                anyhow::bail!(
                    "Page range end ({}) exceeds document length ({})",
                    range.end,
                    page_count
                );
            }
            Ok(range.len())
        }
        None => Ok(page_count),
    }
}

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
            blocks::BlockType::Figure(figure_block) => {
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

                        let output_file = imgs_dir.join(figure_block.path());
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
fn recreate_result_dir(result_dir_name: &Path) -> anyhow::Result<PathBuf> {
    if std::fs::create_dir(result_dir_name).is_err() {
        std::fs::remove_dir_all(result_dir_name)?;
        std::fs::create_dir(result_dir_name)?;
    };
    Ok(result_dir_name.to_owned())
}

pub fn create_dirs<P: AsRef<Path>>(
    output_dir: Option<P>,
    doc_name: &str,
    _debug: bool,
    save_imgs: bool,
) -> anyhow::Result<PathBuf> {
    let result_dir_name = format!("{}-results", sanitize_doc_name(doc_name));
    let res_dir_path = match output_dir {
        Some(p) => {
            let parent_dir = p.as_ref();
            // Create parent directory if it doesn't exist
            std::fs::create_dir_all(parent_dir).with_context(|| {
                format!(
                    "Failed to create output directory: {}",
                    parent_dir.display()
                )
            })?;

            let result_dir_path = parent_dir.join(&result_dir_name);
            recreate_result_dir(&result_dir_path)?
        }
        None => {
            let res_dir_path = PathBuf::from(format!("./{}", &result_dir_name));
            recreate_result_dir(&res_dir_path)?
        }
    };
    if save_imgs {
        let figures_path = res_dir_path.join("figures");
        create_dir(&figures_path).context("cant create figures path")?;
    }

    Ok(res_dir_path)
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
    let doc_json = serde_json::to_string_pretty(&doc)?;
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
        let html_file_out = res_dir_path.join(format!("{sanitized_doc_name}.html"));
        let file = File::create(&html_file_out)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(html_content.as_bytes())?;
    }

    if save_markdown {
        let md_content = to_markdown(doc, &doc.doc_name, Some(fig_path.clone())).unwrap();
        let html_file_out = res_dir_path.join(format!("{sanitized_doc_name}.md"));
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
