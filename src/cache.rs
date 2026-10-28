//! The greeter's cache: a folder of its own under `$XDG_CACHE_HOME`, or
//! `~/.cache`, which for the greeter user lies in its home.

use std::path::PathBuf;

/// The cache folder for `name`, such as Bing's pictures.
pub fn dir(name: &str) -> Option<PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    Some(cache.join("gpui-greeter").join(name))
}
