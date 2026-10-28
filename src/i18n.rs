//! The greeter's words in each language it speaks, and its dates.
//!
//! A new language is one more `Strings` table and its arm in `strings` and
//! `long_date`.

use jiff::Zoned;

use crate::config::Language;

pub struct Strings {
    pub password: &'static str,
    pub press_enter: &'static str,
    pub checking: &'static str,
    pub wrong_password: &'static str,
    pub welcome_back: &'static str,
    /// What separates "Welcome back" from the name.
    pub comma: &'static str,
    pub starting: &'static str,
    pub session: &'static str,
    pub current: &'static str,
    pub sleep: &'static str,
    pub restart: &'static str,
    pub shut_down: &'static str,
    pub click_to_log_in: &'static str,
    /// The drafting sheet's title block.
    pub project: &'static str,
    pub host: &'static str,
    pub no_greetd: &'static str,
    pub confirm: (&'static str, &'static str),
    pub failed: &'static str,
    pub neon: NeonStrings,
}

/// Neon talks like a terminal: lower case, short words.
pub struct NeonStrings {
    pub user: &'static str,
    pub password: &'static str,
    pub placeholder: &'static str,
    pub session: &'static str,
    pub layout: &'static str,
    pub reboot: &'static str,
    pub poweroff: &'static str,
    pub ready: &'static str,
    pub verifying: &'static str,
    pub denied: &'static str,
    pub starting: &'static str,
    pub confirm: (&'static str, &'static str),
}

const ENGLISH: Strings = Strings {
    password: "Password",
    press_enter: "Press Enter to log in",
    checking: "Checking",
    wrong_password: "Wrong password, try again",
    welcome_back: "Welcome back",
    comma: ", ",
    starting: "Starting ",
    session: "Session",
    current: "current",
    sleep: "Sleep",
    restart: "Restart",
    shut_down: "Shut down",
    click_to_log_in: "Click anywhere to log in",
    project: "Project",
    host: "Host",
    no_greetd: "Cannot reach greetd",
    confirm: ("Press ", " again to confirm"),
    failed: " failed: ",
    neon: NeonStrings {
        user: "user",
        password: "password",
        placeholder: "password",
        session: "session",
        layout: "layout",
        reboot: "reboot",
        poweroff: "poweroff",
        ready: "awaiting credentials",
        verifying: "verifying",
        denied: "authentication failed",
        starting: "starting ",
        confirm: ("press ", " again to confirm"),
    },
};

const CHINESE: Strings = Strings {
    password: "密码",
    press_enter: "按回车登录",
    checking: "验证中",
    wrong_password: "密码不对，再试一次",
    welcome_back: "欢迎回来",
    comma: "，",
    starting: "正在启动 ",
    session: "会话",
    current: "当前",
    sleep: "睡眠",
    restart: "重启",
    shut_down: "关机",
    click_to_log_in: "点击任意处登录",
    project: "项目",
    host: "主机",
    no_greetd: "连不上 greetd",
    confirm: ("再按一次", "确认"),
    failed: "失败：",
    neon: NeonStrings {
        user: "用户",
        password: "密码",
        placeholder: "输入密码",
        session: "会话",
        layout: "键盘",
        reboot: "重启",
        poweroff: "关机",
        ready: "等待凭据",
        verifying: "校验中",
        denied: "认证失败，再试一次",
        starting: "正在启动 ",
        confirm: ("再按一次 ", " 确认"),
    },
};

pub fn strings(language: Language) -> &'static Strings {
    match language {
        Language::English => &ENGLISH,
        Language::Chinese => &CHINESE,
    }
}

/// "Tuesday, October 27" or "10月27日　星期二".
pub fn long_date(language: Language, now: &Zoned) -> String {
    let weekday = now.weekday().to_sunday_zero_offset() as usize;
    match language {
        Language::English => {
            const DAYS: [&str; 7] = [
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
            ];
            const MONTHS: [&str; 12] = [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ];
            format!(
                "{}, {} {}",
                DAYS[weekday],
                MONTHS[now.month() as usize - 1],
                now.day()
            )
        }
        Language::Chinese => {
            const DAYS: [&str; 7] = [
                "星期日",
                "星期一",
                "星期二",
                "星期三",
                "星期四",
                "星期五",
                "星期六",
            ];
            format!("{}月{}日\u{3000}{}", now.month(), now.day(), DAYS[weekday])
        }
    }
}

/// "2026-10-27 TUE", the same in every language, as a terminal would print it.
pub fn iso_date(now: &Zoned) -> String {
    const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    let weekday = now.weekday().to_sunday_zero_offset() as usize;
    format!(
        "{:04}-{:02}-{:02} {}",
        now.year(),
        now.month(),
        now.day(),
        DAYS[weekday]
    )
}

/// "09:41", or "09:41:07" with seconds.
pub fn clock(now: &Zoned, seconds: bool) -> String {
    if seconds {
        format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
    } else {
        format!("{:02}:{:02}", now.hour(), now.minute())
    }
}

impl Strings {
    pub fn welcome(&self, name: &str) -> String {
        format!("{}{}{}", self.welcome_back, self.comma, name)
    }

    pub fn confirm(&self, what: &str) -> String {
        format!("{}{}{}", self.confirm.0, what, self.confirm.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuesday() -> Zoned {
        "2026-10-27T09:04:05[America/Los_Angeles]".parse().unwrap()
    }

    #[test]
    fn dates_read_naturally() {
        assert_eq!(
            long_date(Language::English, &tuesday()),
            "Tuesday, October 27"
        );
        assert_eq!(
            long_date(Language::Chinese, &tuesday()),
            "10月27日\u{3000}星期二"
        );
        assert_eq!(iso_date(&tuesday()), "2026-10-27 TUE");
        assert_eq!(clock(&tuesday(), false), "09:04");
        assert_eq!(clock(&tuesday(), true), "09:04:05");
    }

    #[test]
    fn sentences_join_by_language() {
        assert_eq!(
            strings(Language::English).welcome("alice"),
            "Welcome back, alice"
        );
        assert_eq!(
            strings(Language::Chinese).welcome("alice"),
            "欢迎回来，alice"
        );
        assert_eq!(
            strings(Language::English).confirm("Shut down"),
            "Press Shut down again to confirm"
        );
    }
}
