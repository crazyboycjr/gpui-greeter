//! The settings app's words, in the languages the greeter speaks.

use crate::config::{Language, NeonBackground, PaperBackground};

pub struct Words {
    pub title: &'static str,
    pub look: &'static str,
    pub wallpaper: &'static str,
    pub avatar: &'static str,
    pub general: &'static str,

    pub default_mark: &'static str,
    pub use_as_default: &'static str,
    pub default_note: &'static str,
    pub background: &'static str,
    /// Paper's backgrounds and Neon's, in the order of their `ALL`.
    pub paper_backgrounds: [&'static str; 4],
    pub neon_backgrounds: [&'static str; 4],
    pub accent: &'static str,
    pub scanlines: &'static str,
    pub blur: &'static str,
    pub dim: &'static str,
    pub idle: &'static str,
    pub seconds: &'static str,
    pub frost_wallpaper_note: &'static str,
    pub folder_note: &'static str,
    pub folder_broken: &'static str,

    pub show: &'static str,
    pub one_picture: &'static str,
    pub random: &'static str,
    pub daily: &'static str,
    pub bing: &'static str,
    pub one_picture_note: &'static str,
    pub random_note: &'static str,
    pub daily_note: &'static str,
    pub bing_note: &'static str,
    pub pictures: &'static str,
    pub add_pictures: &'static str,
    pub remove: &'static str,
    pub new_mark: &'static str,
    pub no_pictures: &'static str,
    pub kept_in: &'static str,
    pub not_a_picture: &'static str,

    pub choose_picture: &'static str,
    pub avatar_note: &'static str,
    pub avatar_new: &'static str,
    pub no_avatar: &'static str,

    pub size: &'static str,
    pub size_note: &'static str,
    pub language: &'static str,
    pub language_note: &'static str,

    pub sign_in: &'static str,
    pub at_rest: &'static str,
    pub preview_failed: &'static str,

    pub revert: &'static str,
    pub apply: &'static str,
    pub no_changes: &'static str,
    /// "{} changes not applied", the count between the two.
    pub unapplied: (&'static str, &'static str),
    pub one_unapplied: &'static str,
    pub applying: &'static str,
    pub applied: &'static str,
    pub cancelled: &'static str,
    pub failed: &'static str,
    pub unreadable: &'static str,
    pub sudo_title: &'static str,
    pub sudo_note: &'static str,
    pub copy: &'static str,
    pub copied: &'static str,
    pub done: &'static str,
    pub file_dialog_failed: &'static str,
}

const ENGLISH: Words = Words {
    title: "gpui-greeter Settings",
    look: "Look",
    wallpaper: "Wallpaper",
    avatar: "Avatar",
    general: "General",

    default_mark: "default",
    use_as_default: "Use as Default",
    default_note: "The login screen and the lock screen start with the default look; F9 switches looks there.",
    background: "Background",
    paper_backgrounds: ["Desk", "Ledger", "Drafting", "Plain"],
    neon_backgrounds: ["Skyline", "Circuit", "Horizon", "Grid"],
    accent: "Accent",
    scanlines: "Scanlines",
    blur: "Blur",
    dim: "Dim",
    idle: "Clock after",
    seconds: "s",
    frost_wallpaper_note: "Frost shows a wallpaper, chosen under Wallpaper.",
    folder_note: "A theme folder, installed with gpui-greeter install-theme. Its look is its own.",
    folder_broken: "This theme folder does not read; the greeter would show Paper instead. gpui-greeter check-theme says why.",

    show: "Show",
    one_picture: "One Picture",
    random: "Random",
    daily: "Daily",
    bing: "Bing",
    one_picture_note: "Always the picture chosen below; click one to choose it.",
    random_note: "Another picture each time the login screen starts.",
    daily_note: "One picture all day, the next one tomorrow.",
    bing_note: "Bing's picture of the day, which the login screen downloads; one of these until it arrives.",
    pictures: "Pictures",
    add_pictures: "Add Pictures…",
    remove: "Remove",
    new_mark: "new",
    no_pictures: "No pictures yet. Add some for Frost to show.",
    kept_in: "Kept in",
    not_a_picture: "is not a JPEG, PNG or WebP picture",

    choose_picture: "Choose Picture…",
    avatar_note: "The picture is cut to its middle square and set through AccountsService, for your account only.",
    avatar_new: "The new picture is set when you apply.",
    no_avatar: "No picture yet: the looks show your initial.",

    size: "Size",
    size_note: "Every look larger or denser; 100% is as large as macOS's lock screen.",
    language: "Language",
    language_note: "For the login screen, the lock screen and this window.",

    sign_in: "Signing In",
    at_rest: "At Rest",
    preview_failed: "This look cannot be shown.",

    revert: "Revert",
    apply: "Apply",
    no_changes: "No changes",
    unapplied: ("", " changes not applied"),
    one_unapplied: "1 change not applied",
    applying: "Applying…",
    applied: "Applied",
    cancelled: "Cancelled; nothing changed",
    failed: "Not applied: ",
    unreadable: "The settings file has an error, so the greeter uses the defaults: ",
    sudo_title: "Apply in a terminal",
    sudo_note: "polkit could not ask for your password here. Run this in a terminal, then press Done:",
    copy: "Copy",
    copied: "Copied",
    done: "Done",
    file_dialog_failed: "The file dialog did not open: ",
};

const CHINESE: Words = Words {
    title: "登录界面设置",
    look: "外观",
    wallpaper: "壁纸",
    avatar: "头像",
    general: "通用",

    default_mark: "默认",
    use_as_default: "设为默认",
    default_note: "登录界面和锁屏都从默认外观开始；在那里按 F9 可以切换。",
    background: "背景",
    paper_backgrounds: ["书桌", "横格纸", "图纸", "纯色"],
    neon_backgrounds: ["城市夜景", "电路板", "地平线", "网格"],
    accent: "强调色",
    scanlines: "扫描线",
    blur: "模糊",
    dim: "压暗",
    idle: "回到时钟",
    seconds: "秒",
    frost_wallpaper_note: "Frost 显示的壁纸在“壁纸”里选。",
    folder_note: "这是主题文件夹，用 gpui-greeter install-theme 安装，外观由它自己决定。",
    folder_broken: "这个主题文件夹读不出来，登录界面会改用 Paper。用 gpui-greeter check-theme 可以查看原因。",

    show: "显示",
    one_picture: "固定一张",
    random: "每次随机",
    daily: "每天一换",
    bing: "必应壁纸",
    one_picture_note: "总是显示下面选中的那张，点一下图片就能选中。",
    random_note: "登录界面每次启动都换一张。",
    daily_note: "一天显示同一张，第二天换下一张。",
    bing_note: "登录界面会下载必应当天的图片，下载好之前先从下面随机挑一张。",
    pictures: "图片",
    add_pictures: "添加图片…",
    remove: "移除",
    new_mark: "新",
    no_pictures: "还没有图片。添加几张给 Frost 显示吧。",
    kept_in: "图片存放在",
    not_a_picture: "不是 JPEG、PNG 或 WebP 图片",

    choose_picture: "选择图片…",
    avatar_note: "图片会裁成居中的正方形，通过 AccountsService 设为你自己账户的头像。",
    avatar_new: "新头像会在应用后生效。",
    no_avatar: "还没有头像，登录界面会显示你名字的首字母。",

    size: "大小",
    size_note: "所有外观一起放大或缩小；100% 和 macOS 锁屏一样大。",
    language: "语言",
    language_note: "登录界面、锁屏和这个窗口都用这个语言。",

    sign_in: "登录",
    at_rest: "待机",
    preview_failed: "这个外观显示不出来。",

    revert: "还原",
    apply: "应用",
    no_changes: "没有改动",
    unapplied: ("有 ", " 处改动还没应用"),
    one_unapplied: "有 1 处改动还没应用",
    applying: "正在应用…",
    applied: "已应用",
    cancelled: "已取消，什么都没改",
    failed: "没有应用：",
    unreadable: "设置文件有错，登录界面现在用的是默认设置：",
    sudo_title: "在终端里应用",
    sudo_note: "这里没法弹出 polkit 的密码框。请在终端里运行下面这条命令，然后点“好了”：",
    copy: "复制",
    copied: "已复制",
    done: "好了",
    file_dialog_failed: "文件对话框打不开：",
};

pub fn words(language: Language) -> &'static Words {
    match language {
        Language::English => &ENGLISH,
        Language::Chinese => &CHINESE,
    }
}

impl Words {
    pub fn unapplied(&self, count: usize) -> String {
        match count {
            1 => self.one_unapplied.to_string(),
            _ => format!("{}{count}{}", self.unapplied.0, self.unapplied.1),
        }
    }

    pub fn paper_background(&self, background: PaperBackground) -> &'static str {
        let at = PaperBackground::ALL.iter().position(|b| *b == background);
        self.paper_backgrounds[at.unwrap_or(0)]
    }

    pub fn neon_background(&self, background: NeonBackground) -> &'static str {
        let at = NeonBackground::ALL.iter().position(|b| *b == background);
        self.neon_backgrounds[at.unwrap_or(0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_naturally() {
        assert_eq!(
            words(Language::English).unapplied(3),
            "3 changes not applied"
        );
        assert_eq!(
            words(Language::English).unapplied(1),
            "1 change not applied"
        );
        assert_eq!(words(Language::Chinese).unapplied(3), "有 3 处改动还没应用");
        assert_eq!(
            words(Language::Chinese).paper_background(PaperBackground::Ledger),
            "横格纸"
        );
    }
}
