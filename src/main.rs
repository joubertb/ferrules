use ferrules::parse_document;

fn main() {
    let path = "/Users/amine/data/quivr/only_pdfs/0000095.pdf";

    assert!(parse_document(path, None).is_ok())
}
