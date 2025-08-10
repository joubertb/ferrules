use regex::Regex;

fn main() {
    let test_text = "documents #t within the extended context";
    println\!("Original: {}", test_text);
    
    let standalone_regex = Regex::new(r"\b([\!@#$%^&*'\x22])([a-z]+)\b").unwrap();
    
    println\!("Testing standalone regex matches:");
    for cap in standalone_regex.captures_iter(test_text) {
        println\!("  Found: {} (symbol: '{}', suffix: '{}')", &cap[0], &cap[1], &cap[2]);
    }
    
    let result = standalone_regex.replace_all(test_text, |caps: &regex::Captures| {
        let symbol = &caps[1];
        let suffix = &caps[2];
        println\!("Replacing: {}{}", symbol, suffix);
        
        match (symbol, suffix) {
            ("#", "t") => "fit".to_string(),
            _ => format\!("{}{}", symbol, suffix),
        }
    });
    
    println\!("Fixed: {}", result);
}
EOF < /dev/null