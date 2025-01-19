use clap::Parser;
use ferrules::{layout::model::ORTLayoutParser, parse::parse_document, save_parsed_document};
use std::{ops::Range, path::PathBuf};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Ferrules - High-performance document parsing library",
    long_about = "Ferrules is an opinionated high-performance document parsing library designed to generate LLM-ready documents efficiently. Built with Rust for seamless deployment across various platforms."
)]
struct Args {
    /// Path to the PDF file to be parsed
    file_path: PathBuf,

    #[arg(
        long,
        short('r'),
        help = "Specify pages to parse (e.g., '1-5' or '1' for single page)"
    )]
    page_range: Option<String>,

    /// Specifies the target directory where parsing results will be saved
    ///
    /// If not specified, defaults to the current working directory.
    #[arg(
        long,
        env = "FERRULES_OUTPUT_DIR",
        help = "Specify the directory to store parsing result"
    )]
    output_dir: Option<PathBuf>,

    #[arg(
        long,
        default_value_t = false,
        help = "Specify the directory to store parsing result"
    )]
    save_images: bool,

    /// Path to the layout model. If not specified, a default model will be used.
    #[arg(
        long,
        env = "FERRULES_LAYOUT_MODEL_PATH",
        help = "Specify the path to the layout model for document parsing"
    )]
    layout_model_path: Option<PathBuf>,

    /// Use CoreML for layout inference (default: true)
    #[arg(
        long,
        default_value_t = true,
        help = "Enable or disable the use of CoreML for layout inference"
    )]
    pub coreml: bool,

    /// Use CUDA device for layout inference (default: false)
    #[arg(
        long,
        default_value_t = false,
        help = "Enable or disable the use of CUDA for layout inference"
    )]
    pub cuda: bool,

    /// Enable debug mode to output additional information
    #[arg(
        long,
        default_value_t = false,
        env = "FERRULES_DEBUG",
        help = "Activate debug mode for detailed processing information"
    )]
    debug: bool,

    /// Directory for debug output files
    #[arg(
        long,
        env = "FERRULES_DEBUG_PATH",
        help = "Specify the directory to store debug output files"
    )]
    debug_dir: Option<PathBuf>,
}

fn parse_page_range(range_str: &str) -> anyhow::Result<Range<usize>> {
    if let Some((start, end)) = range_str.split_once('-') {
        let start: usize = start.trim().parse()?;
        let end: usize = end.trim().parse()?;
        if start > 0 && end >= start {
            Ok(Range {
                start: start - 1,
                end,
            })
        } else {
            anyhow::bail!("Invalid page range: start must be > 0 and end must be >= start")
        }
    } else {
        // Single page
        let page: usize = range_str.trim().parse()?;
        if page > 0 {
            Ok(Range {
                start: page - 1,
                end: page,
            })
        } else {
            anyhow::bail!("Page number must be greater than 0")
        }
    }
}

fn main() {
    let args = Args::parse();

    let layout_model = ORTLayoutParser::new().expect("can't load layout model");

    let page_range = args
        .page_range
        .map(|page_range_str| parse_page_range(&page_range_str).unwrap());

    let doc = parse_document(
        &args.file_path,
        &layout_model,
        None,
        true,
        page_range,
        args.debug,
    )
    .unwrap();

    save_parsed_document(&doc, args.output_dir.as_ref(), args.save_images).unwrap();
}
