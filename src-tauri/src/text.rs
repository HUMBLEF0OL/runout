//! Free-text validation at the command boundary (TSD §7), mirroring `src/lib/sanitize.ts`.
//! The UI cleans first; Rust never trusts that it did.
// ponytail: no NFC normalisation (would need unicode-normalization); the UI already sends NFC.

/// Length caps in characters; same values as `LIMITS` in `src/lib/sanitize.ts`.
pub mod limit {
    pub const NAME: usize = 200;
    pub const PERSON: usize = 100;
    pub const TAG: usize = 40;
    pub const TEXT: usize = 10_000;
    pub const QUERY: usize = 200;
    pub const LINK: usize = 500;
    pub const FILTER: usize = 100;
    pub const PLAYLIST: usize = 100;
    pub const NOTE: usize = 500;
}

fn invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Single line: control chars → space, invisible/bidi chars dropped, whitespace collapsed, trimmed, capped.
pub fn clean_line(s: &str, max: usize) -> String {
    let s: String = s
        .chars()
        .filter(|c| !invisible(*c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// Multi-line text (reviews, notes): keeps `\n` and `\t`, normalises line endings, max one blank line.
pub fn clean_text(s: &str, max: usize) -> String {
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(s.len());
    for c in s.chars().filter(|c| !invisible(*c)) {
        if c.is_control() && c != '\n' && c != '\t' {
            continue;
        }
        if c == '\n' && out.ends_with("\n\n") {
            continue;
        }
        out.push(c);
    }
    out.trim().chars().take(max).collect()
}

/// Nullable variant: empty after cleaning → `None`.
pub fn line_or_none(s: Option<&str>, max: usize) -> Option<String> {
    s.map(|s| clean_line(s, max)).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_like_the_ui() {
        assert_eq!(clean_line("  a\u{0007}b \u{202E}c\t\nd  ", 50), "a b c d");
        assert_eq!(clean_line("abcdef", 3), "abc");
        assert_eq!(clean_text("x\r\n\r\n\r\n\ry\u{0000}\u{200B}", 50), "x\n\ny");
        assert_eq!(line_or_none(Some(" \u{200B} "), 10), None);
    }
}
