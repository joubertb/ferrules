use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Ferrules - High-performance document parsing library",
    long_about = "Ferrules is an opinionated high-performance document parsing library designed to generate LLM-ready documents efficiently. Built with Rust for seamless deployment across various platforms."
)]
struct Args {
    /// Path to the PDF file to be parsed
    file_path: PathBuf,

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

use ferrules::{layout::model::ORTLayoutParser, parse::parse_document};

fn main() {
    let args = Args::parse();

    // Native
    // let path = "/Users/amine/data/quivr/parsing/native/00b03d60-fe45-4318-a511-18ee921b7bbb.pdf";
    // let path = "/Users/amine/data/quivr/parsing/native/0b0ab5f4-b654-4846-bd9b-18b3c1075c52.pdf";
    // let path = "/Users/amine/data/quivr/parsing/native/0adb1fd6-d009-4097-bcf6-b8f3af38d3f0.pdf";
    //
    // SCANNED
    // let path = "/Users/amine/Downloads/RAG Corporate 2024 016.pdf";
    // let path = "/Users/amine/data/quivr/parsing/scanned/machine.pdf";
    // let path = "/Users/amine/data/quivr/sample-knowledges/2689ade8-2737-4c47-b128-9af369a1cd11.pdf";
    // let path = "/Users/amine/data/quivr/sample-knowledges/6048be22-f0d2-4c83-83ca-0dd1bf8f7336.pdf";

    let layout_model = ORTLayoutParser::new().expect("can't load layout model");

    let doc = parse_document(&args.file_path, &layout_model, None, true, args.debug).unwrap();
    // TODO: Save to output directory
    if args.debug {
        println!("{}", doc.render());
    }
}
