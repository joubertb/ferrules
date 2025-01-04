use ferrules::parse_document;

fn main() {
    let native_single_col =
        "/Users/amine/data/quivr/parsing/native/00b03d60-fe45-4318-a511-18ee921b7bbb.pdf";
    // let native_double_col =
    //     "/Users/amine/data/quivr/parsing/native/0b0ab5f4-b654-4846-bd9b-18b3c1075c52.pdf";
    //
    let path = "/Users/amine/Downloads/RAG Corporate 2024 016.pdf";
    assert!(parse_document(path, None, true).is_ok())
}
