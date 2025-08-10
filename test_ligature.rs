fn main() {
    let test_text = "documents #t within the extended context";
    println!("Original: {}", test_text);
    
    // Test our standalone regex
    use regex::Regex;
    let standalone_regex = Regex::new(r"\b([!@#$%^&*'\x22])([a-z]+)\b").unwrap();
    
    let result = standalone_regex.replace_all(test_text, |caps: &regex::Captures| {
        let symbol = &caps[1];
        let suffix = &caps[2];
        println!("Found standalone pattern: {}{}", symbol, suffix);
        
        match (symbol, suffix) {
            ("#", "t") => "fit".to_string(),
            _ => format!("{}{}", symbol, suffix),
        }
    });
    
    println!("Fixed: {}", result);
    
    // Also test if the word boundary is the issue
    println!("\nTesting word boundaries:");
    let parts: Vec<&str> = test_text.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        println!("Part {}: '{}'", i, part);
        if part == &"#t" {
            println!("  Found exact match for '#t'!");
        }
    }
}