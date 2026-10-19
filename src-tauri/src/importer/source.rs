//! Export input (TSD §4.8): the unzipped folder or the .zip. Files are picked by name only;
//! `YourLibrary.json` and everything else is never opened. Raw files are never copied.
use std::fs::File as FsFile;
use std::io::Read;
use std::path::Path;

use crate::backup::sha256_hex;
use crate::error::{AppError, Code, Result};

/// A single export file above this is refused (largest real ones are ~15 MB; guards zip bombs).
const MAX_FILE: u64 = 256 * 1024 * 1024;

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

// ⚑ T1: names per TSD §4.8 until U-2 confirms them (WI 4).
fn kind(name: &str) -> Option<Kind> {
    let json = name.ends_with(".json");
    if json && name.starts_with("Streaming_History_Audio_") {
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
    r.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(unrecognized("An export file is far larger than expected.").with_detail(name));
    }
    let f = File { name, bytes };
    match k {
        Kind::Streaming => out.streaming.push(f),
        Kind::Playlist => out.playlists.push(f),
    }
    Ok(())
}

fn walk(dir: &Path, out: &mut Export) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, out)?;
        } else if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if kind(name).is_some() {
                push(out, name.to_owned(), FsFile::open(&p)?)?;
            }
        }
    }
    Ok(())
}

/// Read the recognized files of a folder (recursively) or a .zip, sorted by name.
pub fn read(path: &Path) -> Result<Export> {
    let mut out = Export::default();
    if path.is_dir() {
        walk(path, &mut out)?;
    } else if path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        let bad = |e: zip::result::ZipError| {
            unrecognized("That .zip couldn't be read.").with_detail(e.to_string())
        };
        let mut z = zip::ZipArchive::new(FsFile::open(path)?).map_err(bad)?;
        for i in 0..z.len() {
            let f = z.by_index(i).map_err(bad)?;
            let name = f
                .name()
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
            .with_detail("Expected Streaming_History_Audio_*.json or Playlist*.json"));
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
                .all(|n| n.starts_with("Streaming_History_Audio_") || n.starts_with("Playlist")),
            "{names:?}"
        );
        assert_eq!((e.streaming.len(), e.playlists.len()), (2, 1));

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
