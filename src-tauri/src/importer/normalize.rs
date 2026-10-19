//! `norm()` and `norm_key` (TSD §4.6): lower case, diacritics stripped, version suffixes removed.
use std::sync::LazyLock;

use icu_normalizer::DecomposingNormalizerBorrowed;
use regex::Regex;

/// Suffixes that name a version of the same recording, applied to the lower-cased string.
static SUFFIXES: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        // "(feat. X)", "[ft. X]", "(with X)"
        r"\s*[(\[](?:feat\.?|ft\.?|featuring|with)\s[^)\]]*[)\]]",
        // "(Remastered 2009)", "[Live at X]"
        r"\s*[(\[](?:\d{4}\s+)?(?:remaster(?:ed)?|live)\b[^)\]]*[)\]]",
        // " - Remastered 2011", " - 2011 Remaster", " - Live", " - Live at X"
        r"\s+-\s+(?:\d{4}\s+)?(?:remaster(?:ed)?|live)\b.*$",
    ]
    .map(|p| Regex::new(p).expect("valid regex"))
});

/// `lower(strip_diacritics(s))` without version suffixes, whitespace collapsed.
pub fn norm(s: &str) -> String {
    // ponytail: combining marks U+0300–036F only (Latin/Greek/Cyrillic accents); other scripts pass through.
    let mut out = DecomposingNormalizerBorrowed::new_nfd()
        .normalize_iter(s.chars())
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .collect::<String>()
        .to_lowercase();
    for re in SUFFIXES.iter() {
        out = re.replace_all(&out, "").into_owned();
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `norm(artist) | norm(track)`, stored on every `listening_event`.
pub fn norm_key(artist: &str, track: &str) -> String {
    format!("{}|{}", norm(artist), norm(track))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_table() {
        for (input, want) in [
            ("Beyoncé", "beyonce"),
            ("  Ça   Plane Pour Moi ", "ca plane pour moi"),
            ("Song Name - Remastered 2011", "song name"),
            ("Song Name - 2011 Remaster", "song name"),
            ("Song Name (Remastered 2009)", "song name"),
            ("Song Name (feat. Other Artist)", "song name"),
            ("Song Name [ft. Other]", "song name"),
            ("Song Name (with Someone)", "song name"),
            ("Song Name - Live", "song name"),
            ("Song Name - Live at The Hall, 1986", "song name"),
            ("Song Name (feat. X) - Remastered", "song name"),
            ("Live Forever", "live forever"),
            ("Re-Make - Re-Model", "re-make - re-model"),
            ("Without You", "without you"),
            ("", ""),
        ] {
            assert_eq!(norm(input), want, "{input}");
        }
    }

    #[test]
    fn key_joins_artist_and_track() {
        assert_eq!(
            norm_key("Ártist Ñame", "Track (feat. B) - Live"),
            "artist name|track"
        );
    }
}
