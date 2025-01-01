use ferrules::parse_document;

fn main() {
    let path = "/Users/amine/data/quivr/parsing/native/00b03d60-fe45-4318-a511-18ee921b7bbb.pdf";

    assert!(parse_document(path, None, true).is_ok())
}
