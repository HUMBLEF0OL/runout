//! Spotify library save/remove/contains (M5 WI 7): `PUT|DELETE /me/library`,
//! `GET /me/library/contains`, batched ≤ 40 URIs per call, results in input order.
use super::{Spotify, check_uri, client::enc};
use crate::error::{AppError, Result};
use reqwest::Method;

/// Spotify's per-call cap (TSD §5.2).
pub const BATCH: usize = 40;

/// Only tracks and albums (BRD R3-6): an artist URI here means follow, which needs the follow
/// scopes Runout doesn't request (kb/spotify.md G-22). Checked here, at the command boundary.
const KINDS: &[&str] = &["track", "album"];

fn check(uri: &str) -> Result<String> {
    if check_uri(uri, &["artist"]).is_ok() {
        return Err(AppError::validation(
            "Spotify's library takes tracks and albums. Follow artists in Spotify.",
        ));
    }
    check_uri(uri, KINDS)
}

/// One `?uris=a,b,c` path per batch of ≤ 40, in input order. Validates every URI first,
/// so nothing reaches Spotify if one is bad.
// ponytail: `uris` as a comma-separated query param (Feb 2026 Dev Mode guide shape, not yet
// recorded in docs/kb/spotify.md); switch to a JSON body here if Spotify rejects it.
fn paths(base: &str, uris: &[String]) -> Result<Vec<String>> {
    let uris = uris.iter().map(|u| check(u)).collect::<Result<Vec<_>>>()?;
    Ok(uris
        .chunks(BATCH)
        .map(|c| format!("{base}?uris={}", enc(&c.join(","))))
        .collect())
}

/// One bool per URI, same order.
pub async fn library_contains(sp: &Spotify, uris: &[String]) -> Result<Vec<bool>> {
    let mut out = Vec::with_capacity(uris.len());
    for (path, chunk) in paths("/me/library/contains", uris)?
        .iter()
        .zip(uris.chunks(BATCH))
    {
        let got: Vec<bool> = sp.get_json(path).await?;
        if got.len() != chunk.len() {
            return Err(AppError::internal(
                "Spotify answered with an unexpected list.",
            ));
        }
        out.extend(got);
    }
    Ok(out)
}

pub async fn save_to_spotify_library(sp: &Spotify, uris: &[String]) -> Result<()> {
    write(sp, Method::PUT, uris).await
}

pub async fn remove_from_spotify_library(sp: &Spotify, uris: &[String]) -> Result<()> {
    write(sp, Method::DELETE, uris).await
}

async fn write(sp: &Spotify, method: Method, uris: &[String]) -> Result<()> {
    for path in paths("/me/library", uris)? {
        sp.send_json(method.clone(), &path, None).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::testing::*;

    fn uris(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("spotify:track:{i:0>22}")).collect()
    }

    /// Decoded `uris` query of each recorded call.
    fn sent(stub: &Stub) -> Vec<Vec<String>> {
        stub.calls()
            .iter()
            .map(|(_, u)| {
                let q = u.split("?uris=").nth(1).unwrap().replace("%3A", ":");
                q.split("%2C").map(str::to_string).collect()
            })
            .collect()
    }

    #[test]
    fn batches_of_40_in_order() {
        for (n, calls) in [(0, 0), (40, 1), (41, 2), (100, 3)] {
            let d = temp_dir(&format!("lib-save-{n}"));
            let stub = Stub::new(&[]);
            for _ in 0..calls {
                stub.push(200, "");
            }
            let sp = connected(&d, &stub);
            let input = uris(n);
            block(save_to_spotify_library(&sp, &input)).unwrap();
            let got = sent(&stub);
            assert_eq!(got.len(), calls, "n={n}");
            assert!(got.iter().all(|c| c.len() <= BATCH));
            assert_eq!(got.concat(), input, "order kept for n={n}");
            assert!(stub.calls().iter().all(
                |(m, u)| m == "PUT" && u.starts_with("https://api.spotify.com/v1/me/library?")
            ));
        }
    }

    #[test]
    fn remove_uses_delete() {
        let d = temp_dir("lib-del");
        let stub = Stub::new(&[(200, ""), (200, "")]);
        let sp = connected(&d, &stub);
        block(remove_from_spotify_library(&sp, &uris(41))).unwrap();
        assert!(stub.calls().iter().all(|(m, _)| m == "DELETE"));
    }

    #[test]
    fn contains_concatenates_in_input_order() {
        let d = temp_dir("lib-has");
        let first: Vec<bool> = (0..40).map(|i| i % 3 == 0).collect();
        let second: Vec<bool> = (40..100).map(|i| i % 3 == 0).collect();
        let stub = Stub::new(&[
            (200, &serde_json::to_string(&first).unwrap()),
            (200, &serde_json::to_string(&second[..40]).unwrap()),
            (200, &serde_json::to_string(&second[40..]).unwrap()),
        ]);
        let sp = connected(&d, &stub);
        let got = block(library_contains(&sp, &uris(100))).unwrap();
        assert_eq!(got, (0..100).map(|i| i % 3 == 0).collect::<Vec<_>>());
        assert!(stub.calls()[0].1.contains("/me/library/contains?uris="));
        assert!(block(library_contains(&sp, &[])).unwrap().is_empty());
    }

    #[test]
    fn contains_rejects_wrong_length() {
        let d = temp_dir("lib-len");
        let stub = Stub::new(&[(200, "[true]")]);
        let sp = connected(&d, &stub);
        assert!(block(library_contains(&sp, &uris(2))).is_err());
    }

    #[test]
    fn invalid_uri_rejected_before_any_request() {
        let d = temp_dir("lib-bad");
        let stub = Stub::new(&[]);
        let sp = connected(&d, &stub);
        let mut input = uris(41);
        input.push("spotify:playlist:0000000000000000000000".into());
        assert!(block(save_to_spotify_library(&sp, &input)).is_err());
        input[40] = "spotify:track:short".into();
        assert!(block(library_contains(&sp, &input[..41])).is_err());
        // artists: validation for save, remove and contains, before any request
        let artist = vec!["spotify:artist:0000000000000000000000".to_string()];
        for e in [
            block(save_to_spotify_library(&sp, &artist)).unwrap_err(),
            block(remove_from_spotify_library(&sp, &artist)).unwrap_err(),
            block(library_contains(&sp, &artist)).unwrap_err(),
        ] {
            assert_eq!(e.code, crate::error::Code::Validation);
        }
        assert!(stub.calls().is_empty());
    }
}
