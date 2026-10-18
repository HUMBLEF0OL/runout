//! Shared helpers for integration tests: a synthetic library generator (TSD §8: never real
//! values; random 22-char ids, made-up words) and throwaway temp dirs.
#![allow(dead_code)] // each test crate uses a subset
use std::path::PathBuf;

use rand::RngExt;
use rusqlite::{Connection, params};

/// Appears in exactly one review and nowhere else (no syllable below contains q, z or x).
pub const REVIEW_ONLY_WORD: &str = "qzxvolumetrix";
const SYL: &[&str] = &[
    "ba", "ko", "ri", "mel", "dan", "su", "lo", "ve", "tha", "nor", "pi", "gru", "ell", "an", "to",
    "mi", "ra", "sen", "cho", "fu",
];
const ID: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// A fresh empty dir under the OS temp dir.
pub fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("runout-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub struct Gen {
    rng: rand::rngs::ThreadRng,
    vocab: Vec<String>,
}

impl Default for Gen {
    fn default() -> Self {
        let mut rng = rand::rng();
        let vocab = (0..2000)
            .map(|_| {
                (0..rng.random_range(2..4))
                    .map(|_| SYL[rng.random_range(0..SYL.len())])
                    .collect()
            })
            .collect();
        Gen { rng, vocab }
    }
}

impl Gen {
    pub fn id(&mut self) -> String {
        (0..22)
            .map(|_| ID[self.rng.random_range(0..ID.len())] as char)
            .collect()
    }

    pub fn words(&mut self, n: usize) -> String {
        (0..n)
            .map(|_| self.vocab[self.rng.random_range(0..self.vocab.len())].as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Insert one track/album with a review, notes, a tag and a by_artist link; returns its id.
    /// Used by the stress writer (one large transaction per batch).
    pub fn item(&mut self, conn: &Connection, artist: i64, tag: i64) -> i64 {
        let kind = if self.rng.random_range(0..3) == 0 {
            "album"
        } else {
            "track"
        };
        let id: i64 = conn
            .query_row(
                "INSERT INTO item(type, display_name, spotify_uri, review, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
                params![
                    kind,
                    self.words(3),
                    format!("spotify:{kind}:{}", self.id()),
                    self.words(60),
                    self.words(20)
                ],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute("INSERT INTO item_tag VALUES (?1, ?2)", [id, tag])
            .unwrap();
        conn.execute(
            "INSERT INTO item_link VALUES (?1, ?2, 'by_artist')",
            [id, artist],
        )
        .unwrap();
        id
    }
}

/// NFR-04 reference library: `n` items (10% artists, 30% albums, 60% tracks), 300 tags,
/// ~3 tags/links per item in total (by_artist + on_album links, 1–2 tags), reviews/notes of
/// random words on about half. Exactly one review holds [`REVIEW_ONLY_WORD`]; returns that item.
pub fn library(conn: &mut Connection, n: usize) -> i64 {
    let mut g = Gen::default();
    let tx = conn.transaction().unwrap();
    for t in 1..=300 {
        tx.execute(
            "INSERT INTO tag(id, name) VALUES (?1, ?2)",
            params![t, format!("{} {t}", g.words(1))],
        )
        .unwrap();
    }
    let statuses = [
        "planned",
        "in_progress",
        "explored",
        "not_for_me",
        "abandoned",
    ];
    let (n_artists, n_albums) = (n / 10, n * 3 / 10);
    let mut special = 0;
    {
        let mut ins = tx
            .prepare(
                "INSERT INTO item(type, display_name, spotify_uri, status, priority, rating, review,
                   notes, favourite, revisit, in_backlog, recommended_by, backlog_order, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?14))",
            )
            .unwrap();
        let mut tag = tx
            .prepare("INSERT OR IGNORE INTO item_tag VALUES (?1, ?2)")
            .unwrap();
        let mut link = tx
            .prepare("INSERT INTO item_link VALUES (?1, ?2, ?3)")
            .unwrap();
        for i in 0..n {
            let kind = if i < n_artists {
                "artist"
            } else if i < n_artists + n_albums {
                "album"
            } else {
                "track"
            };
            let r = g.rng.random_range(0..100);
            let mut review = (kind != "artist" && r < 50).then(|| g.words(40 + r));
            if i == n / 2 {
                review = Some(format!(
                    "{} {REVIEW_ONLY_WORD} {}",
                    g.words(10),
                    g.words(10)
                ));
            }
            let rec = (r % 7 == 0).then(|| g.words(2));
            let id = ins
                .insert(params![
                    kind,
                    (r % 25 != 0).then(|| g.words(2)), // 4% "name needed"
                    format!("spotify:{kind}:{}", g.id()),
                    statuses[r % 5],
                    ["high", "medium", "low"].get(r % 4).copied(),
                    (r % 3 == 0).then_some(0.5 * (1 + r % 10) as f64),
                    review,
                    (r < 33).then(|| g.words(15)),
                    (r % 11 == 0) as i64,
                    (r % 13 == 0) as i64,
                    (r % 2) as i64,
                    rec,
                    i as i64,
                    format!("-{} minutes", n - i),
                ])
                .unwrap();
            if i == n / 2 {
                special = id;
            }
            for _ in 0..g.rng.random_range(1..3) {
                tag.execute([id, g.rng.random_range(1..=300)]).unwrap();
            }
            if kind != "artist" && n_artists > 0 {
                let a = 1 + g.rng.random_range(0..n_artists as i64);
                link.execute(params![id, a, "by_artist"]).unwrap();
            }
            if kind == "track" && n_albums > 0 {
                let al = 1 + n_artists as i64 + g.rng.random_range(0..n_albums as i64);
                link.execute(params![id, al, "on_album"]).unwrap();
            }
        }
    }
    tx.commit().unwrap();
    special
}
