//! Export input (TSD §4.8): the unzipped folder or the .zip. Files are picked by name only;
//! `YourLibrary.json` and everything else is never opened. Raw files are never copied.
use std::fs::File as FsFile;
use std::io::Read;
use std::path::Path;

use crate::backup::sha256_hex;
use crate::error::{AppError, Code, Result};

/// A single export file above this is refused (largest real ones are ~15 MB; guards zip bombs).
const MAX_FILE: u64 = 256 * 1024 * 1024;
/// All recognized files together (a real lifetime export is ~200 MB unzipped); held in memory.
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
/// Zip entries scanned at most (a real export has a few dozen).
const MAX_ENTRIES: usize = 10_000;
/// Folder levels walked below the chosen folder (the real layout is one level deep).
const MAX_DEPTH: usize = 4;

pub struct File {
    /// Base name, e.g. `Streaming_History_Audio_2024_3.json`.
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
pub struct Export {
    pub streaming: Vec<File>,
    pub playlists: Vec<File>,
}

#[derive(PartialEq)]
enum Kind {
    Streaming,
    Playlist,
}

// T1 verified (TSD §4.8): Video files hold track plays too; rows are filtered by URI, not file kind.
fn kind(name: &str) -> Option<Kind> {
    let json = name.ends_with(".json");
    if json
        && (name.starts_with("Streaming_History_Audio_")
            || name.starts_with("Streaming_History_Video_"))
    {
        Some(Kind::Streaming)
    } else if json && name.starts_with("Playlist") {
        Some(Kind::Playlist)
    } else {
        None
    }
}

pub fn unrecognized(message: &str) -> AppError {
    AppError::new(Code::ImportUnrecognized, message)
}

fn push(out: &mut Export, name: String, r: impl Read) -> Result<()> {
    let Some(k) = kind(&name) else { return Ok(()) };
    let mut bytes = Vec::new();
    // A damaged zip entry (bad CRC, truncated) surfaces here as an io error: name the file (TSD §6).
    // Other io errors (permission denied, file locked) stay io errors: the export isn't at fault.
    r.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                unrecognized("An export file couldn't be read.").with_detail(format!("{name}: {e}"))
            }
            _ => e.into(),
        })?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(unrecognized("An export file is far larger than expected.").with_detail(name));
    }
    let total: u64 = [&out.streaming, &out.playlists]
        .iter()
        .flat_map(|v| v.iter())
        .map(|f| f.bytes.len() as u64)
        .sum::<u64>()
        + bytes.len() as u64;
    if total > MAX_TOTAL {
        return Err(unrecognized("The export is far larger than expected.").with_detail(name));
    }
    let f = File { name, bytes };
    match k {
        Kind::Streaming => out.streaming.push(f),
        Kind::Playlist => out.playlists.push(f),
    }
    Ok(())
}

/// Subfolders that can't be read are skipped; links and junctions are never followed.
fn walk(dir: &Path, depth: usize, out: &mut Export) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) if depth > 0 => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for e in entries.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_symlink() {
            continue; // std reports Windows junctions as symlinks too
        }
        if t.is_dir() {
            if depth < MAX_DEPTH {
                walk(&p, depth + 1, out)?;
            }
        } else if let Some(name) = p.file_name().and_then(|n| n.to_str())
            && kind(name).is_some()
        {
            push(out, name.to_owned(), FsFile::open(&p)?)?;
        }
    }
    Ok(())
}

/// Read the recognized files of a folder (recursively) or a .zip, sorted by name.
pub fn read(path: &Path) -> Result<Export> {
    let mut out = Export::default();
    if path.is_dir() {
        walk(path, 0, &mut out)?;
    } else if path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        let bad = |e: zip::result::ZipError| {
            unrecognized("That .zip couldn't be read.").with_detail(e.to_string())
        };
        let mut z = zip::ZipArchive::new(FsFile::open(path)?).map_err(bad)?;
        if z.len() > MAX_ENTRIES {
            return Err(unrecognized(
                "That .zip holds far more files than an export.",
            ));
        }
        for i in 0..z.len() {
            let f = z.by_index(i).map_err(bad)?;
            let name = f
                .name()
                .map_err(bad)?
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .to_owned();
            if f.is_file() {
                push(&mut out, name, f)?;
            }
        }
    } else {
        return Err(AppError::validation(
            "Choose the export folder or its .zip file.",
        ));
    }
    if out.streaming.is_empty() && out.playlists.is_empty() {
        return Err(unrecognized("No Spotify export files found there.")
            .with_detail("Expected Streaming_History_Audio_*.json, Streaming_History_Video_*.json or Playlist*.json"));
    }
    out.streaming.sort_by(|a, b| a.name.cmp(&b.name));
    out.playlists.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

impl Export {
    pub fn names(&self) -> Vec<String> {
        self.streaming
            .iter()
            .chain(&self.playlists)
            .map(|f| f.name.clone())
            .collect()
    }
}

/// Content checksum of a file set (order-independent, names ignored): `import_batch.checksum`.
pub fn checksum(kind: &str, files: &[File]) -> String {
    let mut hashes: Vec<String> = files.iter().map(|f| sha256_hex(&f.bytes)).collect();
    hashes.sort();
    sha256_hex(format!("{kind}|{}", hashes.join("|")).as_bytes())
}

/// The synthetic export (structure only, made-up values; TSD §8).
#[cfg(test)]
pub fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/export_synthetic")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn detects_by_name_in_folder_and_zip() {
        let e = read(&fixture()).unwrap();
        let names = e.names();
        assert!(
            names
                .iter()
                .all(|n| n.starts_with("Streaming_History_") || n.starts_with("Playlist")),
            "{names:?}"
        );
        assert_eq!((e.streaming.len(), e.playlists.len()), (3, 1));

        // same files zipped inside a subfolder → same set, same checksum
        let zp = std::env::temp_dir().join(format!("runout-src-{}.zip", std::process::id()));
        let mut w = zip::ZipWriter::new(FsFile::create(&zp).unwrap());
        for f in std::fs::read_dir(fixture()).unwrap() {
            let p = f.unwrap().path();
            let name = format!(
                "Spotify Extended Streaming History/{}",
                p.file_name().unwrap().to_str().unwrap()
            );
            w.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(&std::fs::read(&p).unwrap()).unwrap();
        }
        w.finish().unwrap();
        let z = read(&zp).unwrap();
        assert_eq!(z.names(), names);
        assert_eq!(checksum("s", &z.streaming), checksum("s", &e.streaming));
        let _ = std::fs::remove_file(zp);

        let empty = std::env::temp_dir().join(format!("runout-src-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::write(empty.join("YourLibrary.json"), "{}").unwrap();
        assert_eq!(read(&empty).err().unwrap().code, Code::ImportUnrecognized);
        assert_eq!(
            read(&empty.join("nope.txt")).err().unwrap().code,
            Code::Validation
        );
        let _ = std::fs::remove_dir_all(empty);
    }
}

#[cfg(test)]
mod walk_tests {
    use super::*;

    #[test]
    fn folder_walk_stops_at_max_depth() {
        let root = std::env::temp_dir().join(format!("runout-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ok = root.join("Spotify Extended Streaming History");
        let deep = (0..=MAX_DEPTH).fold(root.clone(), |p, i| p.join(format!("d{i}")));
        for d in [&ok, &deep] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("Streaming_History_Audio_2020.json"), "[]").unwrap();
        }
        let e = read(&root).unwrap();
        assert_eq!(e.streaming.len(), 1, "only the shallow file is read");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A damaged entry inside the .zip (CRC mismatch) or truncated JSON names the file
    /// (`import_unrecognized`, TSD §6), never a bare I/O error.
    #[test]
    fn damaged_zip_entry_names_the_file() {
        use std::io::Write;
        let body = br#"[{"ts": "2026-01-01T00:00:00Z", "ms_played": 1000}]"#;
        let zp = std::env::temp_dir().join(format!("runout-src-bad-{}.zip", std::process::id()));
        let mut w = zip::ZipWriter::new(FsFile::create(&zp).unwrap());
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("x/Streaming_History_Audio_2026.json", opts)
            .unwrap();
        w.write_all(body).unwrap();
        w.finish().unwrap();
        let mut bytes = std::fs::read(&zp).unwrap();
        let at = bytes
            .windows(body.len())
            .position(|win| win == body)
            .unwrap();
        bytes[at + 10] ^= 0x55; // corrupt the stored data; the CRC no longer matches
        std::fs::write(&zp, &bytes).unwrap();
        let e = read(&zp).err().unwrap();
        let _ = std::fs::remove_file(&zp);
        assert_eq!(e.code, Code::ImportUnrecognized, "{e:?}");
        assert_eq!(
            e.detail
                .as_deref()
                .map(|d| d.starts_with("Streaming_History_Audio_2026.json")),
            Some(true),
            "{e:?}"
        );
    }
}
