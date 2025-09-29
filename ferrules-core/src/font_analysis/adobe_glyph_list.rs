//! Adobe Glyph List standard mappings
//!
//! Provides the standard mapping from glyph names to Unicode characters
//! as defined by Adobe's Glyph List Specification.
//!
//! This is used to determine the correct Unicode character for any given
//! glyph name found in PDF fonts.

use phf::Map;

/// Adobe Glyph List mapping from glyph name to Unicode character or string
///
/// This contains the standard mappings as defined by Adobe for determining
/// the correct Unicode representation of PDF font glyphs.
/// Updated to support ligatures that expand to multiple characters.
/// Uses compile-time perfect hash for O(1) lookup performance.
pub static ADOBE_GLYPH_LIST: Map<&'static str, &'static str> = phf::phf_map! {
    // Essential ASCII characters
    "A" => "A",
    "B" => "B",
    "C" => "C",
    "D" => "D",
    "E" => "E",
    "F" => "F",
    "G" => "G",
    "H" => "H",
    "I" => "I",
    "J" => "J",
    "K" => "K",
    "L" => "L",
    "M" => "M",
    "N" => "N",
    "O" => "O",
    "P" => "P",
    "Q" => "Q",
    "R" => "R",
    "S" => "S",
    "T" => "T",
    "U" => "U",
    "V" => "V",
    "W" => "W",
    "X" => "X",
    "Y" => "Y",
    "Z" => "Z",

    "a" => "a",
    "b" => "b",
    "c" => "c",
    "d" => "d",
    "e" => "e",
    "f" => "f",
    "g" => "g",
    "h" => "h",
    "i" => "i",
    "j" => "j",
    "k" => "k",
    "l" => "l",
    "m" => "m",
    "n" => "n",
    "o" => "o",
    "p" => "p",
    "q" => "q",
    "r" => "r",
    "s" => "s",
    "t" => "t",
    "u" => "u",
    "v" => "v",
    "w" => "w",
    "x" => "x",
    "y" => "y",
    "z" => "z",

    // Common ligatures (complete strings to fix corruption at source)
    // NOTE: "fi" ligature mapping disabled due to incorrect insertion in words like "findfiings"
    // TODO: Re-enable with proper context validation if legitimate fi ligatures are needed
    // "fi" => "fi", // fi ligature → complete "fi" string (DISABLED - causes word corruption)
    "fl" => "fl", // fl ligature → complete "fl" string
    "ff" => "ff", // ff ligature → complete "ff" string
    "ffi" => "ffi", // ffi ligature → complete "ffi" string
    "ffl" => "ffl", // ffl ligature → complete "ffl" string

    // Numbers
    "zero" => "0",
    "one" => "1",
    "two" => "2",
    "three" => "3",
    "four" => "4",
    "five" => "5",
    "six" => "6",
    "seven" => "7",
    "eight" => "8",
    "nine" => "9",

    // Essential punctuation and symbols
    "space" => " ",
    "exclam" => "!",
    "quotedbl" => "\"",
    "numbersign" => "#",
    "dollar" => "$",
    "percent" => "%",
    "ampersand" => "&",
    "quotesingle" => "'",
    "parenleft" => "(",
    "parenright" => ")",
    "angleleft" => "⟨",    // U+27E8 - Mathematical Left Angle Bracket
    "angleright" => "⟩",   // U+27E9 - Mathematical Right Angle Bracket
    "asterisk" => "*",
    "plus" => "+",
    "comma" => ",",
    "hyphen" => "-",
    "period" => ".",
    "slash" => "/",
    "colon" => ":",
    "semicolon" => ";",
    "less" => "<",
    "equal" => "=",
    "greater" => ">",
    "question" => "?",
    "at" => "@",
    "bracketleft" => "[",
    "backslash" => "\\",
    "bracketright" => "]",
    "asciicircum" => "^",
    "underscore" => "_",
    "grave" => "`",
    "braceleft" => "{",
    "bar" => "|",
    "braceright" => "}",
    "asciitilde" => "~",

    // Mathematical symbols commonly found in academic papers
    "minus" => "−", // U+2212 (different from hyphen)
    "multiply" => "×", // U+00D7
    "divide" => "÷", // U+00F7
    "plusminus" => "±", // U+00B1

    // Superscript numbers (for mathematical formulas like E=mc²)
    "twosuperior" => "²", // U+00B2
    "threesuperior" => "³", // U+00B3

    // Greek letters commonly used in mathematics
    "alpha" => "α",
    "beta" => "β",
    "gamma" => "γ",
    "delta" => "δ",
    "epsilon" => "ε",
    "pi" => "π",
    "sigma" => "σ",
};

/// Look up the correct Unicode character or string for a glyph name
pub fn get_unicode_for_glyph(glyph_name: &str) -> Option<String> {
    ADOBE_GLYPH_LIST.get(glyph_name).map(|s| s.to_string())
}
