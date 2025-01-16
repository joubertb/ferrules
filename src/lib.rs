use colored::*;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use entities::Document;
use serde::Serialize;

pub mod parse;

pub mod blocks;
pub mod entities;
pub mod layout;

#[cfg(target_os = "macos")]
pub mod ocr;

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

pub fn save_parsed_document<P: AsRef<Path> + Serialize>(
    doc: &Document<P>,
    output_dir: Option<P>,
) -> anyhow::Result<()> {
    let output_name = format!("{}-results.json", doc.doc_name);
    let output_path = match output_dir {
        Some(p) => p.as_ref().to_owned().join(&output_name),
        None => format!("./{}", &output_name).into(),
    };
    let file = File::create(&output_path)?;
    let mut writer = BufWriter::new(file);
    let doc_json = serde_json::to_string(&doc)?;
    writer.write_all(doc_json.as_bytes())?;

    if let Some(dbg_path) = &doc.debug_path {
        println!(
            "{} Debug output saved in: {}",
            "ℹ".yellow().bold(),
            dbg_path.display().to_string().yellow().underline()
        );
    }

    println!(
        "{} Results saved in: {}",
        "✓".green().bold(),
        output_path.display().to_string().cyan().underline()
    );

    Ok(())
}
