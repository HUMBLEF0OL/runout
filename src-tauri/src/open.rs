//! "Open Spotify" (FR-044 attribution, M6 acceptance "every item opens in the Spotify app"). Every
//! new-window request from the WebView ends here: the UI's `target="_blank"` links and the embeds' own
//! links. Tauri drops them otherwise. Nothing opens inside Runout; the OS opens the Spotify app or browser.
use tauri::webview::NewWindowResponse;
use tauri::{Runtime, Url};

use crate::links::{self, LinkState};

/// What to hand the OS, in order, for a requested URL: an album/track/artist link goes to the Spotify app
/// (`spotify:` URI) with its https page as the fallback; any other https page on a spotify.com host opens
/// in the browser; everything else is refused.
pub fn targets(url: &Url) -> Vec<String> {
    let spotify_host = url
        .host_str()
        .is_some_and(|h| h == "spotify.com" || h.ends_with(".spotify.com"));
    if url.scheme() != "https" || !spotify_host {
        return vec![];
    }
    let p = links::parse(url.as_str());
    match (p.state, p.uri) {
        (LinkState::Ok, Some(uri)) => vec![uri, url.to_string()],
        _ => vec![url.to_string()],
    }
}

/// `on_new_window` handler: open outside, never create a WebView window.
pub fn handle<R: Runtime>(url: Url) -> NewWindowResponse<R> {
    for t in targets(&url) {
        if shell_open(&t) {
            break;
        }
    }
    NewWindowResponse::Deny
}

/// Open with the registered handler; false when nothing handles it (no Spotify app for `spotify:`).
#[cfg(windows)]
fn shell_open(target: &str) -> bool {
    use windows::Win32::UI::Shell::{
        ASSOCF_NONE, ASSOCSTR_PROGID, AssocQueryStringW, ShellExecuteW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PWSTR, w};
    let scheme = target.split(':').next().unwrap_or_default();
    // Unregistered schemes make Windows offer the Store instead of failing: check the association first.
    // PROGID, not EXECUTABLE: the Microsoft Store Spotify registers `spotify:` as an AppX ProgID with no
    // executable (seen on the dev PC, 20 Oct 2026); a scheme nobody registered fails both.
    let mut len = 0u32;
    let registered = unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_PROGID,
            &HSTRING::from(scheme),
            w!("open"),
            None::<PWSTR>,
            &mut len,
        )
    }
    .is_ok();
    registered
        && unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &HSTRING::from(target),
                None,
                None,
                SW_SHOWNORMAL,
            )
        }
        .0 as isize
            > 32
}

#[cfg(not(windows))]
fn shell_open(_target: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(u: &str) -> Vec<String> {
        targets(&Url::parse(u).unwrap())
    }

    #[test]
    fn app_first_for_library_links_browser_for_other_spotify_pages_nothing_else() {
        let id = "4aawyAB9vmqN3uQ7FjRGTy";
        assert_eq!(
            t(&format!("https://open.spotify.com/album/{id}?si=x")),
            [
                format!("spotify:album:{id}"),
                format!("https://open.spotify.com/album/{id}?si=x")
            ]
        );
        assert_eq!(
            t("https://open.spotify.com/"),
            ["https://open.spotify.com/"]
        );
        assert_eq!(
            t("https://accounts.spotify.com/x"),
            ["https://accounts.spotify.com/x"]
        );
        assert!(t("http://open.spotify.com/album/x").is_empty());
        assert!(t("https://localhost/open.spotify.com").is_empty());
        assert!(t("https://127.0.0.1/").is_empty());
        assert!(t("file:///C:/Windows/System32/calc.exe").is_empty());
    }
}
