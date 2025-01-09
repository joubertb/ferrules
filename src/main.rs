use std::time::{self, Instant};

use ferrules::parse::parse_document;

fn main() {
    let path = "/Users/amine/data/quivr/parsing/native/00b03d60-fe45-4318-a511-18ee921b7bbb.pdf";
    // let path = "/Users/amine/data/quivr/parsing/native/0b0ab5f4-b654-4846-bd9b-18b3c1075c52.pdf";
    // let path = "/Users/amine/Downloads/RAG Corporate 2024 016.pdf";

    let start_time = time::Instant::now();
    parse_document(path, None, true).unwrap();
    println!(
        "Parsing doc took : {} ms",
        Instant::now().duration_since(start_time).as_millis()
    )
}
