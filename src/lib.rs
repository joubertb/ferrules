pub mod parse;

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
