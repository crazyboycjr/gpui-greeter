//! The avatar: a picture cut to its middle square and scaled down, and
//! AccountsService, which keeps it where the greeter reads it and lets
//! each user set their own, no root needed.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};
use image::ImageFormat;

use crate::wallpaper::Picture;

/// Avatars are this many pixels across: the looks draw them at most 72
/// points, which is 144 pixels on the densest monitors.
pub const SIZE: u32 = 256;

/// Where AccountsService keeps each user's avatar, by name.
pub const ICONS: &str = "/var/lib/AccountsService/icons";

/// The picture at `path` as an avatar: its middle square, [`SIZE`] pixels
/// across, as PNG.
pub fn square_png(path: &Path) -> Result<Vec<u8>> {
    let picture = Picture::open(path)?;
    let square = picture.cover((SIZE, SIZE))?;
    let mut bytes = Vec::new();
    square.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(bytes)
}

/// The avatar `user` has now, in the folder `icons`.
pub fn current(icons: &Path, user: &str) -> Option<PathBuf> {
    Some(icons.join(user)).filter(|path| path.is_file())
}

/// What sets a user's avatar: AccountsService, or a stand-in for tests.
pub trait Accounts: Send + Sync {
    /// Makes the PNG at `png` the avatar of `user`.
    fn set_icon(&self, user: &str, png: &Path) -> Result<()>;
}

/// AccountsService on the system bus, asked as the user. polkit lets an
/// active session's user change their own avatar, and asks for a password
/// in its own dialog where the system wants one.
pub struct AccountsService;

impl Accounts for AccountsService {
    fn set_icon(&self, user: &str, png: &Path) -> Result<()> {
        const ACCOUNTS: &str = "org.freedesktop.Accounts";
        let png = png.to_str().context("the avatar's path is not text")?;
        // Long enough for someone to type a password into polkit's dialog.
        let bus = zbus::blocking::connection::Builder::system()?
            .method_timeout(Duration::from_secs(300))
            .build()
            .context("cannot reach the system bus")?;
        let reply = bus
            .call_method(
                Some(ACCOUNTS),
                "/org/freedesktop/Accounts",
                Some(ACCOUNTS),
                "FindUserByName",
                &(user,),
            )
            .context("AccountsService does not answer")?;
        let account: zbus::zvariant::OwnedObjectPath = reply.body().deserialize()?;
        let account =
            zbus::blocking::Proxy::new(&bus, ACCOUNTS, account, "org.freedesktop.Accounts.User")?;
        account
            .call_with_flags::<_, _, ()>(
                "SetIconFile",
                zbus::proxy::MethodFlags::AllowInteractiveAuth.into(),
                &(png,),
            )
            .context("AccountsService did not take the avatar")?;
        Ok(())
    }
}

/// Keeps avatars in a folder, as AccountsService keeps them in its own:
/// for the settings app's test runs, which must not touch the system's.
pub struct IconsFolder(pub PathBuf);

impl Accounts for IconsFolder {
    fn set_icon(&self, user: &str, png: &Path) -> Result<()> {
        std::fs::create_dir_all(&self.0)?;
        std::fs::copy(png, self.0.join(user))
            .with_context(|| format!("cannot keep the avatar in {}", self.0.display()))?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records what it is asked, and sets nothing.
    #[derive(Default)]
    pub struct Recorder(pub Mutex<Vec<(String, PathBuf)>>);

    impl Accounts for Recorder {
        fn set_icon(&self, user: &str, png: &Path) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .push((user.to_string(), png.to_path_buf()));
            Ok(())
        }
    }

    #[test]
    fn cuts_the_middle_square() {
        let dir = std::env::temp_dir().join(format!("gpui-greeter-avatar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A wide picture, red in its middle third, blue either side.
        let wide = image::RgbaImage::from_fn(900, 300, |x, _| {
            image::Rgba(if (300..600).contains(&x) {
                [220, 30, 30, 255]
            } else {
                [30, 30, 220, 255]
            })
        });
        let path = dir.join("wide.png");
        wide.save(&path).unwrap();
        let png = square_png(&path).unwrap();
        let square = image::load_from_memory(&png).unwrap().into_rgba8();
        assert_eq!(square.dimensions(), (SIZE, SIZE));
        for (x, y) in [(2, 2), (SIZE / 2, SIZE / 2), (SIZE - 3, SIZE - 3)] {
            let pixel = square.get_pixel(x, y).0;
            assert!(pixel[0] > 200 && pixel[2] < 60, "{x},{y}: {pixel:?}");
        }
        assert!(square_png(&dir.join("missing.png")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folder_stands_in_for_accounts_service() {
        let dir = std::env::temp_dir().join(format!("gpui-greeter-icons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("new.png");
        std::fs::write(&png, b"png").unwrap();
        let icons = dir.join("icons");
        assert_eq!(current(&icons, "alice"), None);
        IconsFolder(icons.clone()).set_icon("alice", &png).unwrap();
        assert_eq!(current(&icons, "alice"), Some(icons.join("alice")));
        let recorder = Recorder::default();
        recorder.set_icon("alice", &png).unwrap();
        assert_eq!(recorder.0.lock().unwrap()[0], ("alice".into(), png));
        let _ = std::fs::remove_dir_all(dir);
    }
}
