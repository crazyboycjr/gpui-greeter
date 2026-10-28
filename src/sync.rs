//! `gpui-greeter sync-wallpapers`: copies the user's wallpapers where the
//! greeter, which runs as a user of its own, can read them.
//!
//! The destination mirrors the source each time it runs: new pictures are
//! added, changed ones replaced and removed ones removed. Files that are not
//! pictures are skipped, and a picture saved twice is copied once.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};

use crate::wallpaper::{is_picture_name, picture_format};

#[derive(Debug, Default)]
pub struct Report {
    pub copied: Vec<String>,
    pub unchanged: usize,
    pub removed: Vec<String>,
    pub duplicates: Vec<String>,
    pub not_pictures: Vec<String>,
}

pub fn sync(from: &Path, to: &Path) -> Result<Report> {
    let source = from
        .canonicalize()
        .with_context(|| format!("cannot open {}", from.display()))?;
    std::fs::create_dir_all(to).with_context(|| format!("cannot create {}", to.display()))?;
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o755))?;
    ensure!(
        to.canonicalize()? != source,
        "{} is the folder being copied",
        to.display()
    );

    let mut report = Report::default();
    let mut names: Vec<String> = std::fs::read_dir(&source)?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    // The shortest of a picture's names is kept: "a.jpg" over "a (copy).jpg".
    names.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));

    let mut seen: HashMap<u64, Vec<(String, Vec<u8>)>> = HashMap::new();
    let mut wanted: Vec<(String, Vec<u8>)> = Vec::new();
    for name in names {
        let path = source.join(&name);
        let Some(format) = sniff(&path)? else {
            report.not_pictures.push(name);
            continue;
        };
        let bytes = std::fs::read(&path)?;
        let copies = seen.entry(fingerprint(&bytes)).or_default();
        if copies.iter().any(|(_, other)| *other == bytes) {
            report.duplicates.push(name);
            continue;
        }
        let target = unique_name(&name, format, &wanted);
        copies.push((target.clone(), bytes.clone()));
        wanted.push((target, bytes));
    }

    for (name, bytes) in &wanted {
        let path = to.join(name);
        if std::fs::read(&path).is_ok_and(|old| old == *bytes) {
            report.unchanged += 1;
            continue;
        }
        write_atomically(&path, bytes)?;
        report.copied.push(name.clone());
    }

    for entry in std::fs::read_dir(to)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let ours = entry.file_type().is_ok_and(|kind| kind.is_file()) && is_picture_name(&name);
        if ours && !wanted.iter().any(|(kept, _)| *kept == name) {
            std::fs::remove_file(entry.path())?;
            report.removed.push(name);
        }
    }
    Ok(report)
}

/// The picture format of the file at `path`, from its first bytes.
fn sniff(path: &Path) -> Result<Option<&'static str>> {
    let mut head = [0u8; 12];
    let read = std::fs::File::open(path)?.read(&mut head)?;
    Ok(picture_format(&head[..read]))
}

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// The file's name with a picture extension if it has none, made distinct
/// from the names already taken.
fn unique_name(name: &str, format: &str, taken: &[(String, Vec<u8>)]) -> String {
    let base = if is_picture_name(name) {
        name.to_string()
    } else {
        format!("{name}.{format}")
    };
    let free = |candidate: &str| !taken.iter().any(|(other, _)| other == candidate);
    if free(&base) {
        return base;
    }
    let (stem, ext) = base.rsplit_once('.').unwrap_or((&base, format));
    (2..)
        .map(|n| format!("{stem}-{n}.{ext}"))
        .find(|candidate| free(candidate))
        .unwrap()
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let partial = PathBuf::from(format!("{}.partial", path.display()));
    std::fs::write(&partial, bytes)?;
    std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o644))?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest-of-png";

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gpui-greeter-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn mirrors_pictures_once() {
        let from = scratch("from");
        let to = scratch("to");
        std::fs::write(from.join("a.jpg"), JPEG).unwrap();
        std::fs::write(from.join("a (copy).jpg"), JPEG).unwrap();
        std::fs::write(from.join("dunes_3c6"), PNG).unwrap();
        std::fs::write(from.join("notes.txt"), b"not a picture").unwrap();
        std::fs::write(to.join("gone.png"), PNG).unwrap();

        let report = sync(&from, &to).unwrap();
        assert_eq!(report.copied, ["a.jpg", "dunes_3c6.png"]);
        assert_eq!(report.duplicates, ["a (copy).jpg"]);
        assert_eq!(report.not_pictures, ["notes.txt"]);
        assert_eq!(report.removed, ["gone.png"]);

        let again = sync(&from, &to).unwrap();
        assert!(again.copied.is_empty() && again.removed.is_empty());
        assert_eq!(again.unchanged, 2);
        let _ = std::fs::remove_dir_all(from);
        let _ = std::fs::remove_dir_all(to);
    }

    #[test]
    fn names_stay_distinct() {
        let taken = vec![("x.jpg".to_string(), Vec::new())];
        assert_eq!(unique_name("x", "jpg", &taken), "x-2.jpg");
        assert_eq!(unique_name("y.PNG", "png", &taken), "y.PNG");
    }
}
