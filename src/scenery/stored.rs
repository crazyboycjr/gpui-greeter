//! Pictures drawn before, one for each window size, kept with a fingerprint
//! of what they show and of the program that drew them: a start that wants
//! the same picture reads it back instead of drawing it again, which takes
//! a good part of a second.

use std::fs::{self, File};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Stored {
    path: PathBuf,
    fingerprint: u64,
    /// The picture's size in bytes, four to a pixel.
    len: usize,
}

impl Stored {
    /// The drawing kept for the scenery called `name`, at this size.
    pub fn new(
        dir: &Path,
        name: &str,
        svg: &str,
        (width, height): (u32, u32),
        grain: bool,
    ) -> Self {
        Self::at(
            dir.join(format!("{name}-{width}x{height}.rgba")),
            &[svg.as_bytes(), &[u8::from(grain)]],
            (width, height),
        )
    }

    /// A picture of `width` by `height` kept at `path`, drawn from `parts`.
    pub fn at(path: PathBuf, parts: &[&[u8]], (width, height): (u32, u32)) -> Self {
        let mut all: Vec<&[u8]> = parts.to_vec();
        let (w, h, stamp) = (width.to_le_bytes(), height.to_le_bytes(), build_stamp());
        all.extend([&w[..], &h[..], &stamp[..]]);
        Self {
            path,
            fingerprint: fingerprint(&all),
            len: width as usize * height as usize * 4,
        }
    }

    pub fn load(&self) -> Option<Vec<u8>> {
        let mut file = File::open(&self.path).ok()?;
        let mut stamp = [0; 8];
        file.read_exact(&mut stamp).ok()?;
        if u64::from_le_bytes(stamp) != self.fingerprint {
            return None;
        }
        let mut pixels = Vec::with_capacity(self.len);
        file.read_to_end(&mut pixels).ok()?;
        (pixels.len() == self.len).then_some(pixels)
    }

    pub fn save(&self, pixels: &[u8]) {
        if let Err(error) = self.write(pixels) {
            log::warn!(
                "cannot keep the scenery in {}: {error}",
                self.path.display()
            );
        }
    }

    fn write(&self, pixels: &[u8]) -> io::Result<()> {
        // Windows of one size draw the same picture, maybe at the same time.
        static WRITES: AtomicUsize = AtomicUsize::new(0);
        let partial = self.path.with_extension(format!(
            "{}-{}.partial",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = File::create(&partial)?;
        file.write_all(&self.fingerprint.to_le_bytes())?;
        file.write_all(pixels)?;
        fs::rename(&partial, &self.path)
    }
}

/// When this program was built, by its file's time: a new build draws
/// afresh, whatever it changed.
fn build_stamp() -> [u8; 16] {
    std::env::current_exe()
        .and_then(fs::metadata)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or([0; 16], |since| since.as_nanos().to_le_bytes())
}

/// FNV-1a over `parts` in turn.
pub(super) fn fingerprint(parts: &[&[u8]]) -> u64 {
    parts
        .iter()
        .flat_map(|part| part.iter())
        .fold(0xCBF2_9CE4_8422_2325, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_only_the_same_picture() {
        let dir = std::env::temp_dir().join(format!("gpui-greeter-stored-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let desk = Stored::new(&dir, "paper-desk", "<svg>desk</svg>", (2, 1), true);
        assert_eq!(desk.load(), None);
        desk.save(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(desk.load(), Some(vec![1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(
            Stored::new(&dir, "paper-desk", "<svg>ledger</svg>", (2, 1), true).load(),
            None
        );
        assert_eq!(
            Stored::new(&dir, "paper-desk", "<svg>desk</svg>", (2, 1), false).load(),
            None
        );
        // Another look's drawing, kept beside it, leaves it be.
        let night = Stored::new(&dir, "paper-night-desk", "<svg>night</svg>", (2, 1), true);
        night.save(&[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(desk.load(), Some(vec![1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(night.load(), Some(vec![8, 7, 6, 5, 4, 3, 2, 1]));
        let _ = fs::remove_dir_all(dir);
    }
}
