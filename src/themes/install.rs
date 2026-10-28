//! `gpui-greeter install-theme`: checks a theme folder and copies it where
//! the greeter reads themes, owned by whoever runs it (root, with sudo) and
//! readable by everyone, and can pick it in the settings, keeping their
//! comments.
//!
//! Only regular files inside the folder are copied, never what a link
//! points to, so a theme cannot carry other files along.

use std::fs;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};

use super::files::MAX_FILE;
use super::{Folder, Problem, RESERVED, Severity, valid_name};

const MAX_FILES: usize = 4_096;
const MAX_TOTAL: u64 = 512 << 20;
const MAX_DEPTH: usize = 16;

/// Files a theme may carry besides theme.toml: what it draws, its fonts,
/// and the texts that say where they come from.
const EXTENSIONS: [&str; 12] = [
    "toml", "svg", "png", "jpg", "jpeg", "webp", "ttf", "otf", "ttc", "txt", "md", "license",
];
const NAMES: [&str; 6] = [
    "LICENSE", "COPYING", "README", "SOURCES", "AUTHORS", "NOTICE",
];

pub struct Installed {
    pub name: String,
    pub dir: PathBuf,
    pub copied: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<Problem>,
}

/// The name a folder installs as: `name`, or the folder's own.
pub fn theme_name(from: &Path, name: Option<&str>) -> Result<String> {
    let name = match name {
        Some(name) => name.to_string(),
        None => from
            .canonicalize()
            .with_context(|| format!("cannot open {}", from.display()))?
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
    };
    ensure!(
        valid_name(&name),
        "{name:?} cannot name a theme: use lower-case letters, digits, - and _ (--name NAME)"
    );
    ensure!(
        !RESERVED.contains(&name.as_str()),
        "{name:?} is a built-in look's name; pick another with --name"
    );
    Ok(name)
}

/// Checks the theme in `from`, then copies it to `to/<name>`, replacing an
/// earlier copy only once the new one is complete and checked.
pub fn install(from: &Path, to: &Path, name: &str) -> Result<Installed> {
    let from = from
        .canonicalize()
        .with_context(|| format!("cannot open {}", from.display()))?;
    if let Err(problems) = Folder::load(&from) {
        bail!("{}", refusal(&from, &problems));
    }
    let (files, skipped) = collect(&from)?;

    fs::create_dir_all(to).with_context(|| format!("cannot create {}", to.display()))?;
    fs::set_permissions(to, fs::Permissions::from_mode(0o755))?;
    let to = to.canonicalize()?;
    ensure!(
        !to.starts_with(&from) && !from.starts_with(to.join(name)),
        "{} and {} overlap",
        from.display(),
        to.display()
    );
    let staging = to.join(format!(".{name}.partial-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let copied = copy(&from, &staging, &files);
    if let Err(error) = copied {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    // What is read now are the copies, which the user can no longer change.
    let warnings = match Folder::load(&staging) {
        Ok((_, warnings)) => warnings,
        Err(problems) => {
            let _ = fs::remove_dir_all(&staging);
            bail!("{}", refusal(&from, &problems));
        }
    };
    let target = to.join(name);
    let old = to.join(format!(".{name}.old-{}", std::process::id()));
    if target.exists() {
        fs::rename(&target, &old)
            .with_context(|| format!("cannot move the earlier {} aside", target.display()))?;
    }
    if let Err(error) = fs::rename(&staging, &target) {
        if old.exists() {
            let _ = fs::rename(&old, &target);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(error).with_context(|| format!("cannot put the theme in {}", target.display()));
    }
    if old.exists() {
        fs::remove_dir_all(&old).with_context(|| format!("cannot remove {}", old.display()))?;
    }
    Ok(Installed {
        name: name.to_string(),
        dir: target,
        copied: files,
        skipped,
        warnings,
    })
}

fn refusal(from: &Path, problems: &[Problem]) -> String {
    let errors: Vec<String> = problems
        .iter()
        .filter(|problem| problem.severity == Severity::Error)
        .map(ToString::to_string)
        .collect();
    format!(
        "{} is not ready to install:\n{}",
        from.display(),
        errors.join("\n")
    )
}

/// The files to copy, relative to `root`, and what is left behind.
fn collect(root: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    let mut total = 0u64;
    let mut pending = vec![(PathBuf::new(), 0usize)];
    while let Some((relative, depth)) = pending.pop() {
        let mut entries: Vec<_> =
            fs::read_dir(root.join(&relative))?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = relative.join(&name);
            let shown = path.to_string_lossy().into_owned();
            let kind = entry.file_type()?;
            if name.starts_with('.') {
                skipped.push(format!("{shown} (hidden)"));
            } else if kind.is_symlink() {
                skipped.push(format!("{shown} (a link)"));
            } else if kind.is_dir() {
                if depth + 1 >= MAX_DEPTH {
                    bail!("{shown}: folders nest deeper than {MAX_DEPTH}");
                }
                pending.push((path, depth + 1));
            } else if !kind.is_file() {
                skipped.push(format!("{shown} (not a regular file)"));
            } else if !wanted(&name) {
                skipped.push(format!("{shown} (not a kind of file themes use)"));
            } else {
                let size = entry.metadata()?.len();
                ensure!(
                    size <= MAX_FILE,
                    "{shown} is larger than {} MiB",
                    MAX_FILE >> 20
                );
                total += size;
                ensure!(
                    total <= MAX_TOTAL,
                    "the theme is larger than {} MiB",
                    MAX_TOTAL >> 20
                );
                files.push(shown);
                ensure!(
                    files.len() <= MAX_FILES,
                    "the theme has more than {MAX_FILES} files"
                );
            }
        }
    }
    files.sort();
    Ok((files, skipped))
}

fn wanted(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default();
    let extension = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase);
    match extension {
        Some(extension) => EXTENSIONS.contains(&extension.as_str()),
        None => NAMES.contains(&stem),
    }
}

/// Copies each file as it is now, refusing one that turned into a link or
/// grew since it was listed.
fn copy(from: &Path, to: &Path, files: &[String]) -> Result<()> {
    fs::create_dir(to).with_context(|| format!("cannot create {}", to.display()))?;
    fs::set_permissions(to, fs::Permissions::from_mode(0o755))?;
    for relative in files {
        let source = from.join(relative);
        let listed = fs::symlink_metadata(&source)?;
        ensure!(
            listed.file_type().is_file(),
            "{relative} changed while being copied"
        );
        let mut input = fs::File::open(&source)?;
        let opened = input.metadata()?;
        ensure!(
            opened.is_file() && opened.dev() == listed.dev() && opened.ino() == listed.ino(),
            "{relative} changed while being copied"
        );
        let mut bytes = Vec::new();
        (&mut input).take(MAX_FILE + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE,
            "{relative} grew while being copied"
        );
        let target = to.join(relative);
        if let Some(parent) = target.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
            let mut dir = parent.to_path_buf();
            while dir.starts_with(to) && dir != to {
                fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))?;
                dir.pop();
            }
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&target)
            .with_context(|| format!("cannot write {}", target.display()))?;
        output.write_all(&bytes)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

/// Sets `theme = "<name>"` in the settings at `config`, keeping the rest of
/// the file as it is, comments included.
pub fn select(config: &Path, name: &str) -> Result<()> {
    let text = match fs::read_to_string(config) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", config.display()));
        }
    };
    let edited = with_theme(&text, name)?;
    toml::from_str::<crate::config::Config>(&edited)
        .with_context(|| format!("{} would not read back", config.display()))?;
    let partial = PathBuf::from(format!("{}.partial", config.display()));
    if let Some(parent) = config.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&partial, edited)?;
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o644))?;
    fs::rename(&partial, config).with_context(|| format!("cannot replace {}", config.display()))?;
    Ok(())
}

fn with_theme(text: &str, name: &str) -> Result<String> {
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .context("the settings are not valid TOML; fix them first")?;
    match document
        .get_mut("theme")
        .and_then(|item| item.as_value_mut())
    {
        Some(value) => {
            let decor = value.decor().clone();
            *value = name.into();
            *value.decor_mut() = decor;
        }
        None => {
            document.insert("theme", toml_edit::value(name));
        }
    }
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_theme_and_keeps_comments() {
        let before = "# Settings.\n\n# The look.\ntheme = \"paper\" # for now\nlanguage = \"en\"\n\n[frost]\nblur = 20\n";
        let after = with_theme(before, "console").unwrap();
        assert_eq!(
            after,
            "# Settings.\n\n# The look.\ntheme = \"console\" # for now\nlanguage = \"en\"\n\n[frost]\nblur = 20\n"
        );
        let added = with_theme("[frost]\nblur = 20\n", "console").unwrap();
        assert!(added.starts_with("theme = \"console\"\n"), "{added}");
        let config: crate::config::Config = toml::from_str(&added).unwrap();
        assert_eq!(config.theme, crate::config::Theme::Folder("console".into()));
        assert_eq!(config.frost.blur, 20.0);
    }

    #[test]
    fn copies_only_regular_files_inside() {
        let base =
            std::env::temp_dir().join(format!("gpui-greeter-install-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let theme = base.join("dial");
        fs::create_dir_all(theme.join("art/.git")).unwrap();
        fs::write(
            theme.join("theme.toml"),
            "format = 1\n[[layer]]\nkind = \"svg\"\nfile = \"art/dot.svg\"\nwidth = 10\n",
        )
        .unwrap();
        fs::write(
            theme.join("art/dot.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><circle cx="2" cy="2" r="2"/></svg>"#,
        )
        .unwrap();
        fs::write(theme.join("SOURCES.md"), "art: drawn for this theme\n").unwrap();
        fs::write(theme.join("art/.git/config"), "secret").unwrap();
        fs::write(theme.join("notes.sh"), "#!/bin/sh").unwrap();
        fs::write(base.join("private.txt"), "private").unwrap();
        std::os::unix::fs::symlink(base.join("private.txt"), theme.join("art/stolen.txt")).unwrap();

        let to = base.join("themes");
        let installed = install(&theme, &to, "dial").unwrap();
        assert_eq!(
            installed.copied,
            ["SOURCES.md", "art/dot.svg", "theme.toml"]
        );
        assert!(
            installed
                .skipped
                .iter()
                .any(|s| s.starts_with("art/stolen.txt"))
        );
        assert!(!to.join("dial/art/stolen.txt").exists());
        assert!(!to.join("dial/notes.sh").exists());
        let mode = fs::metadata(to.join("dial/art/dot.svg"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);

        // A second install replaces the first, and a broken theme is refused.
        fs::write(theme.join("SOURCES.md"), "changed").unwrap();
        install(&theme, &to, "dial").unwrap();
        assert_eq!(
            fs::read_to_string(to.join("dial/SOURCES.md")).unwrap(),
            "changed"
        );
        fs::write(
            theme.join("theme.toml"),
            "format = 1\n[[layer]]\nkind = \"blob\"\n",
        )
        .unwrap();
        assert!(install(&theme, &to, "dial").is_err());
        assert_eq!(
            fs::read_to_string(to.join("dial/SOURCES.md")).unwrap(),
            "changed"
        );
        assert!(theme_name(&theme, Some("frost")).is_err());
        assert!(theme_name(&theme, Some("../x")).is_err());
        assert_eq!(theme_name(&theme, None).unwrap(), "dial");
        let _ = fs::remove_dir_all(base);
    }
}
