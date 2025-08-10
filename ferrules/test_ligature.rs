use ferrules_core::entities::*;

fn main() {
    let test_cases = [
        "long-context",     // Should NOT be changed
        "work!ows",         // Should become "workflows"
        "speci#c",          // Should become "specific"
        "e\"ective",        // Should become "effective"
        "longficontext",    // Already corrupted - should remain as is if no pattern matches
        "long context",     // No hyphen - should not match
        "long–context",     // Different dash character
        "Recent advances in long-context LLMs", // Full sentence test
    ];
    
    for test in &test_cases {
        // We need to test the function that's actually being called
        println!("Testing: '{}'", test);
        
        // Simulate what happens in the Line::append function
        let result = fix_utf8_corruption(test);
        println!("  Result: '{}'", result);
        
        if test != &result {
            println!("  CHANGED!");
        } else {
            println!("  No change (as expected)");
        }
        println!();
    }
}