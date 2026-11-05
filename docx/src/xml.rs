//! Writing XML text and attribute values.
//!
//! ```text
//! text("a < b & c")   ->  "a &lt; b &amp; c"
//! attr(r#"say "hi""#) ->  "say &quot;hi&quot;"
//! text("a\u{1}b")     ->  "ab"         XML 1.0 has no spelling for C0 controls
//! ```

pub const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

pub fn text(s: &str) -> String {
    escape(s, false)
}

pub fn attr(s: &str) -> String {
    escape(s, true)
}

/// Can this character appear in an XML 1.0 document at all?
pub fn allowed(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

fn escape(s: &str, quote: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quote => out.push_str("&quot;"),
            c if allowed(c) => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        assert_eq!(text("a < b & c"), "a &lt; b &amp; c");
        assert_eq!(attr(r#"say "hi""#), "say &quot;hi&quot;");
        assert_eq!(text("a\u{1}b"), "ab");
    }
}
