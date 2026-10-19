//! M4 SMTC spike (plan step 1): follow the Spotify media session for N seconds (default 60) and
//! print each finished track with its accumulated `ms_played` (pauses excluded), using the logger's
//! own `session::Watcher` and `accumulator`. Writes nothing. Real track names go to the console only.
//!
//! `cargo run --example smtc_spike -- 60`
fn main() {
    #[cfg(windows)]
    spike();
    #[cfg(not(windows))]
    eprintln!("SMTC is Windows-only");
}

#[cfg(windows)]
fn spike() {
    use runout_lib::logger::{accumulator::Accumulator, poll::POLL, session::Watcher};
    use std::time::{Duration, Instant};

    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(60);
    let (tx, rx) = std::sync::mpsc::channel();
    let mut w = Watcher::new(tx).expect("SMTC session manager");
    let apps = vec!["Spotify".to_string()];
    let mut acc = Accumulator::default();
    let (mut wakes, mut events, mut last) = (0, 0, None);
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        wakes += 1;
        if rx.recv_timeout(POLL).is_ok() {
            events += 1;
        }
        let snap = match w.snapshot(&apps) {
            Ok(s) => s,
            Err(e) => {
                println!("snapshot error: {e}");
                continue;
            }
        };
        let shown = snap.as_ref().map(|(t, s)| (t.title.clone(), *s));
        if shown != last {
            println!("now: {shown:?}");
            last = shown;
        }
        if let Some(c) = acc.observe(snap, chrono::Utc::now()) {
            println!(
                "closed: {} / {} ms_played={}",
                c.track.title, c.track.artist, c.ms_played
            );
        }
    }
    if let Some(c) = acc.close(chrono::Utc::now()) {
        println!(
            "closed at exit: {} ms_played={}",
            c.track.title, c.ms_played
        );
    }
    println!("wakes={wakes} smtc_events={events} (the rest were 5 s polls)");
}
