use std::time::{self, Instant};

use ferrules::{layout::model::ORTLayoutParser, parse::parse_document};

fn main() {
    // Native
    // let path = "/Users/amine/data/quivr/parsing/native/00b03d60-fe45-4318-a511-18ee921b7bbb.pdf";
    // let path = "/Users/amine/data/quivr/parsing/native/0b0ab5f4-b654-4846-bd9b-18b3c1075c52.pdf";
    let path = "/Users/amine/data/quivr/parsing/native/0adb1fd6-d009-4097-bcf6-b8f3af38d3f0.pdf";
    // SCANNED
    // let path = "/Users/amine/Downloads/RAG Corporate 2024 016.pdf";
    // let path = "/Users/amine/data/quivr/parsing/scanned/machine.pdf";

    let layout_model =
        ORTLayoutParser::new("./models/yolov8s-doclaynet.onnx").expect("can't load layout model");

    for _ in 0..1 {
        let start_time = time::Instant::now();
        let doc = parse_document(path, &layout_model, None, true).unwrap();

        println!("{}", doc.render());

        println!(
            "Parsing doc took : {} ms",
            Instant::now().duration_since(start_time).as_millis()
        )
    }
}
