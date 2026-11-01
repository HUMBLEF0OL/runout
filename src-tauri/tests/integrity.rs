//! NFR-05: kill a process mid-write and mid-backup; the library still passes `integrity_check`
//! and opens. The child is this test binary re-run on `stress_child` (nothing ships in the app).
//! `TerminateProcess` = a crash, not power loss: OS buffers survive (see docs/kb/perf-integrity.md).
mod common;

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use runout_lib::{backup, db};
use rusqlite::Connection;

const BATCH: i64 = 500;

/// Child side. Runs only when spawned by `kill_at` (env set); a no-op in normal `--ignored` runs.
#[test]
#[ignore]
fn stress_child() {
    let (Ok(mode), Ok(path)) = (
        std::env::var("RUNOUT_STRESS"),
        std::env::var("RUNOUT_STRESS_DB"),
    ) else {
        return;
    };
    let mut conn = db::open(Path::new(&path)).unwrap();
    println!("go");
    if mode == "backup" {
        let dir = Path::new(&path).parent().unwrap().join("backups");
        loop {
            backup::backup_now(&conn, &dir, 50).unwrap();
        }
    }
    let mut g = common::Gen::default();
    let artist: i64 = conn
        .query_row(
            "INSERT INTO item(type, display_name) VALUES ('artist', 'Stress Artist') RETURNING id",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO tag(id, name) VALUES (1, 'stress')",
        [],
    )
    .unwrap();
    loop {
        // One large write per transaction: BATCH items + FTS rows + tags + links.
        let tx = conn.transaction().unwrap();
        for _ in 0..BATCH {
            g.item(&tx, artist, 1);
        }
        tx.commit().unwrap();
    }
}

/// Spawn `stress_child` in `mode` on `db_path`, kill it `ms` after it reports "go".
fn kill_at(mode: &str, db_path: &Path, ms: u64) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "stress_child", "--nocapture"])
        .env("RUNOUT_STRESS", mode)
        .env("RUNOUT_STRESS_DB", db_path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    while line.trim() != "go" {
        line.clear();
        assert!(out.read_line(&mut line).unwrap() > 0, "child exited early");
    }
    std::thread::sleep(Duration::from_millis(ms));
    child.kill().unwrap();
    child.wait().unwrap();
}

/// Raw open (no migrate) → `integrity_check` + FTS5 integrity, then the app's `db::open` works.
fn assert_intact(path: &Path) -> i64 {
    {
        let c = Connection::open(path).unwrap();
        let ok: String = c
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ok, "ok");
        c.execute(
            "INSERT INTO item_fts(item_fts) VALUES ('integrity-check')",
            [],
        )
        .unwrap();
    }
    let c = db::open(path).unwrap();
    c.query_row(
        "SELECT count(*) FROM item WHERE type <> 'artist'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

fn write_kill(offsets: &[u64]) {
    let dir = common::temp_dir("kill-write");
    let path = dir.join("library.db");
    drop(db::open(&path).unwrap());
    let mut last = 0;
    for &ms in offsets {
        kill_at("write", &path, ms);
        let n = assert_intact(&path);
        // Transactions are atomic: only whole batches ever land.
        assert_eq!(n % BATCH, 0, "partial batch after kill at {ms} ms");
        assert!(n >= last);
        println!("kill mid-write at {ms} ms: ok, {n} items committed");
        last = n;
    }
}

/// Quick variant (debug build, a few seconds).
#[test]
fn kill_mid_write_keeps_library_intact() {
    write_kill(&[300]);
}

#[test]
#[ignore]
fn kill_mid_write_three_offsets() {
    write_kill(&[400, 1300, 2700]);
}

fn backup_kill(name: &str, items: usize, offsets: &[u64]) {
    let dir = common::temp_dir(name);
    let path = dir.join("library.db");
    let backups = dir.join("backups");
    common::library(&mut db::open(&path).unwrap(), items);
    for &ms in offsets {
        kill_at("backup", &path, ms);
        assert_intact(&path);
        // Whatever was half-written is never listed, and every listed backup verifies.
        let listed = backup::list_backups(&backups, 50);
        for b in &listed {
            assert!(backup::check_backup(Path::new(&b.path), &path).unwrap().ok);
        }
        println!(
            "kill mid-backup at {ms} ms: ok, {} backups listed",
            listed.len()
        );
    }
    // A truncated zip under a backup name is skipped, not an error.
    let real = backup::list_backups(&backups, 50);
    if let Some(b) = real.first() {
        let bytes = std::fs::read(&b.path).unwrap();
        std::fs::write(
            backups.join("runout-backup-19990101-000000.zip"),
            &bytes[..bytes.len() / 2],
        )
        .unwrap();
    }
    assert_eq!(backup::list_backups(&backups, 50).len(), real.len());
    // The next good backup clears killed leftovers (.zip.tmp, .<name>.db).
    let conn = db::open(&path).unwrap();
    backup::backup_now(&conn, &backups, 50).unwrap();
    let stray: Vec<_> = std::fs::read_dir(&backups)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp") || n.ends_with(".db"))
        .collect();
    assert!(stray.is_empty(), "leftovers: {stray:?}");
    drop(conn);
}

/// Quick variant (debug build, a few seconds): runs in check.py, so mid-backup NFR-05 is always proven.
#[test]
fn kill_mid_backup_keeps_library_intact() {
    backup_kill("kill-backup-quick", 1_000, &[100]);
}

#[test]
#[ignore]
fn kill_mid_backup_three_offsets() {
    backup_kill("kill-backup", 10_000, &[30, 250, 700]);
}
