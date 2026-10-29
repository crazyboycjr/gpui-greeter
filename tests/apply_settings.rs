//! `gpui-greeter apply-settings` as pkexec runs it: a request on standard
//! input, here against a test folder standing in for `/`, which only a
//! debug build accepts.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use gpui_greeter::config::{Config, Pick, Theme};
use gpui_greeter::settings::request::{NewPicture, Request};
use gpui_greeter::settings::setting::Setting;

/// A folder with the shipped settings, pointing at its own wallpaper,
/// themes and Bing folders.
fn root(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("apply-settings-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("etc/gpui-greeter")).unwrap();
    std::fs::create_dir_all(root.join("wallpapers")).unwrap();
    let shipped = include_str!("../dist/config.toml")
        .replace(
            "/usr/local/share/gpui-greeter/wallpapers",
            &root.join("wallpapers").display().to_string(),
        )
        .replace(
            "/usr/local/share/gpui-greeter/themes",
            &root.join("themes").display().to_string(),
        )
        .replace(
            "/usr/local/share/gpui-greeter/bing4k",
            &root.join("bing4k").display().to_string(),
        );
    std::fs::write(root.join("etc/gpui-greeter/config.toml"), shipped).unwrap();
    root
}

fn run(root: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gpui-greeter"))
        .arg("apply-settings")
        .arg("--root")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(input);
    child.wait_with_output().unwrap()
}

fn encoded(request: &Request) -> Vec<u8> {
    let mut bytes = Vec::new();
    request.write(&mut bytes).unwrap();
    bytes
}

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(16, 9, image::Rgba([90, 60, 140, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

#[test]
fn applies_a_request_from_standard_input() {
    let root = root("apply");
    let request = Request {
        settings: vec![
            Setting::Theme(Theme::Frost),
            Setting::WallpaperPick(Pick::File("lake.png".into())),
        ],
        add: vec![NewPicture {
            name: "lake.png".into(),
            bytes: png(),
        }],
        remove: vec![],
        bing_timer: None,
    };
    let output = run(&root, &[], &encoded(&request));
    assert!(output.status.success(), "{output:?}");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains("added    lake.png"), "{said}");
    let config = Config::load(&root.join("etc/gpui-greeter/config.toml"));
    assert_eq!(config.theme, Theme::Frost);
    assert_eq!(config.wallpaper.pick, Pick::File("lake.png".into()));
    assert_eq!(
        std::fs::read(root.join("wallpapers/lake.png")).unwrap(),
        png()
    );
}

#[test]
fn refuses_anything_else_and_changes_nothing() {
    let root = root("refuse");
    let config = root.join("etc/gpui-greeter/config.toml");
    let before = std::fs::read(&config).unwrap();
    let header = |json: &str| format!("gpui-greeter settings 1\n{json}\n").into_bytes();
    let cases: [(&str, Vec<u8>, &[&str]); 9] = [
        ("garbage", b"rm -rf /\n".to_vec(), &[]),
        (
            "the session command",
            header(r#"{"set":{"sessions.wrapper":"sh -c id"}}"#),
            &[],
        ),
        (
            "another wallpaper folder",
            header(r#"{"set":{"wallpaper.dir":"/etc"}}"#),
            &[],
        ),
        (
            "a picture outside the folder",
            header(r#"{"add":[{"name":"../../x.png","size":0}]}"#),
            &[],
        ),
        (
            "a removal outside the folder",
            header(r#"{"remove":["../etc/gpui-greeter/config.toml"]}"#),
            &[],
        ),
        (
            "a picture that is not one",
            [
                header(r#"{"add":[{"name":"x.png","size":5}]}"#),
                b"hello".to_vec(),
            ]
            .concat(),
            &[],
        ),
        (
            "a settings file of its own choosing",
            encoded(&Request {
                settings: vec![Setting::Theme(Theme::Neon)],
                ..Request::default()
            }),
            &["--config", "/tmp/elsewhere.toml"],
        ),
        (
            "a Bing picture that is not there",
            encoded(&Request {
                settings: vec![Setting::WallpaperPick(Pick::BingFile(
                    "2026-10-29.jpg".into(),
                ))],
                ..Request::default()
            }),
            &[],
        ),
        (
            "a timer that is not installed",
            encoded(&Request {
                settings: vec![Setting::Theme(Theme::Neon)],
                bing_timer: Some(true),
                ..Request::default()
            }),
            &[],
        ),
    ];
    for (why, input, args) in cases {
        let output = run(&root, args, &input);
        assert!(!output.status.success(), "accepted {why}: {output:?}");
        assert!(!output.stderr.is_empty(), "refused {why} without a word");
    }
    assert_eq!(std::fs::read(&config).unwrap(), before);
    let left: Vec<_> = std::fs::read_dir(root.join("wallpapers"))
        .unwrap()
        .collect();
    assert!(left.is_empty(), "{left:?}");
}
