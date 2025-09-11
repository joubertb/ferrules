//! Adobe Glyph List standard mappings
//!
//! Provides the standard mapping from glyph names to Unicode characters
//! as defined by Adobe's Glyph List Specification.
//!
//! This is used to determine the correct Unicode character for any given
//! glyph name found in PDF fonts.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Adobe Glyph List mapping from glyph name to Unicode character
///
/// This contains the standard mappings as defined by Adobe for determining
/// the correct Unicode representation of PDF font glyphs.
pub static ADOBE_GLYPH_LIST: Lazy<HashMap<&'static str, char>> = Lazy::new(|| {
    let mut map = HashMap::new();

    // Essential ASCII characters
    map.insert("A", 'A');
    map.insert("B", 'B');
    map.insert("C", 'C');
    map.insert("D", 'D');
    map.insert("E", 'E');
    map.insert("F", 'F');
    map.insert("G", 'G');
    map.insert("H", 'H');
    map.insert("I", 'I');
    map.insert("J", 'J');
    map.insert("K", 'K');
    map.insert("L", 'L');
    map.insert("M", 'M');
    map.insert("N", 'N');
    map.insert("O", 'O');
    map.insert("P", 'P');
    map.insert("Q", 'Q');
    map.insert("R", 'R');
    map.insert("S", 'S');
    map.insert("T", 'T');
    map.insert("U", 'U');
    map.insert("V", 'V');
    map.insert("W", 'W');
    map.insert("X", 'X');
    map.insert("Y", 'Y');
    map.insert("Z", 'Z');

    map.insert("a", 'a');
    map.insert("b", 'b');
    map.insert("c", 'c');
    map.insert("d", 'd');
    map.insert("e", 'e');
    map.insert("f", 'f');
    map.insert("g", 'g');
    map.insert("h", 'h');
    map.insert("i", 'i');
    map.insert("j", 'j');
    map.insert("k", 'k');
    map.insert("l", 'l');
    map.insert("m", 'm');
    map.insert("n", 'n');
    map.insert("o", 'o');
    map.insert("p", 'p');
    map.insert("q", 'q');
    map.insert("r", 'r');
    map.insert("s", 's');
    map.insert("t", 't');
    map.insert("u", 'u');
    map.insert("v", 'v');
    map.insert("w", 'w');
    map.insert("x", 'x');
    map.insert("y", 'y');
    map.insert("z", 'z');

    // Numbers
    map.insert("zero", '0');
    map.insert("one", '1');
    map.insert("two", '2');
    map.insert("three", '3');
    map.insert("four", '4');
    map.insert("five", '5');
    map.insert("six", '6');
    map.insert("seven", '7');
    map.insert("eight", '8');
    map.insert("nine", '9');

    // Essential punctuation and symbols
    map.insert("space", ' ');
    map.insert("exclam", '!');
    map.insert("quotedbl", '"');
    map.insert("numbersign", '#');
    map.insert("dollar", '$');
    map.insert("percent", '%');
    map.insert("ampersand", '&');
    map.insert("quotesingle", '\'');
    map.insert("parenleft", '(');
    map.insert("parenright", ')');
    map.insert("asterisk", '*');
    map.insert("plus", '+');
    map.insert("comma", ',');
    map.insert("hyphen", '-');
    map.insert("period", '.');
    map.insert("slash", '/');
    map.insert("colon", ':');
    map.insert("semicolon", ';');
    map.insert("less", '<');
    map.insert("equal", '=');
    map.insert("greater", '>');
    map.insert("question", '?');
    map.insert("at", '@');
    map.insert("bracketleft", '[');
    map.insert("backslash", '\\');
    map.insert("bracketright", ']');
    map.insert("asciicircum", '^');
    map.insert("underscore", '_');
    map.insert("grave", '`');
    map.insert("braceleft", '{');
    map.insert("bar", '|');
    map.insert("braceright", '}');
    map.insert("asciitilde", '~');

    // Mathematical symbols commonly found in academic papers
    map.insert("minus", '−'); // U+2212 (different from hyphen)
    map.insert("multiply", '×'); // U+00D7
    map.insert("divide", '÷'); // U+00F7
    map.insert("plusminus", '±'); // U+00B1

    // Superscript numbers (for mathematical formulas like E=mc²)
    map.insert("twosuperior", '²'); // U+00B2
    map.insert("threesuperior", '³'); // U+00B3

    // Greek letters commonly used in mathematics
    map.insert("alpha", 'α');
    map.insert("beta", 'β');
    map.insert("gamma", 'γ');
    map.insert("delta", 'δ');
    map.insert("epsilon", 'ε');
    map.insert("pi", 'π');
    map.insert("sigma", 'σ');

    map
});

/// Look up the correct Unicode character for a glyph name
pub fn get_unicode_for_glyph(glyph_name: &str) -> Option<char> {
    ADOBE_GLYPH_LIST.get(glyph_name).copied()
}
