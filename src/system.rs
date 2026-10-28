//! What the screen says about the machine: its name, system, seat and
//! terminal, how long it has been up and whether it is online.

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Machine {
    pub hostname: String,
    /// The distribution's name in lower case, e.g. "arch linux".
    pub os: String,
    pub kernel: String,
    pub seat: String,
    /// The virtual terminal greetd runs the greeter on.
    pub vt: Option<String>,
}

impl Machine {
    pub fn read() -> Self {
        let proc = |key: &str| {
            std::fs::read_to_string(format!("/proc/sys/kernel/{key}"))
                .map(|text| text.trim().to_string())
                .unwrap_or_default()
        };
        Self {
            hostname: proc("hostname"),
            os: os_name(&std::fs::read_to_string("/etc/os-release").unwrap_or_default()),
            kernel: proc("osrelease"),
            seat: std::env::var("XDG_SEAT").unwrap_or_else(|_| "seat0".into()),
            vt: std::env::var("XDG_VTNR").ok().map(|vt| format!("tty{vt}")),
        }
    }
}

fn os_name(os_release: &str) -> String {
    os_release
        .lines()
        .find_map(|line| line.strip_prefix("NAME="))
        .map(|name| name.trim_matches('"').to_lowercase())
        .unwrap_or_else(|| "linux".into())
}

pub fn uptime() -> Option<Duration> {
    let text = std::fs::read_to_string("/proc/uptime").ok()?;
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_secs_f64(seconds))
}

/// "21h 12m".
pub fn format_uptime(uptime: Duration) -> String {
    let minutes = uptime.as_secs() / 60;
    format!("{}h {:02}m", minutes / 60, minutes % 60)
}

/// Whether a default route is up, over IPv4 or IPv6.
pub fn online() -> bool {
    let ipv4 = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
    let ipv6 = std::fs::read_to_string("/proc/net/ipv6_route").unwrap_or_default();
    has_default_route(&ipv4, &ipv6)
}

fn has_default_route(ipv4: &str, ipv6: &str) -> bool {
    const UP: u32 = 0x1;
    let v4 = ipv4.lines().skip(1).any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        fields.len() > 3
            && fields[1] == "00000000"
            && u32::from_str_radix(fields[3], 16).is_ok_and(|flags| flags & UP != 0)
    });
    let v6 = ipv6.lines().any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        fields.len() == 10
            && fields[0].bytes().all(|b| b == b'0')
            && fields[1] == "00"
            && fields[9] != "lo"
            && u32::from_str_radix(fields[8], 16).is_ok_and(|flags| flags & UP != 0)
    });
    v4 || v6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_system() {
        assert_eq!(os_name("NAME=\"Arch Linux\"\nID=arch\n"), "arch linux");
        assert_eq!(os_name(""), "linux");
    }

    #[test]
    fn finds_a_default_route() {
        let header =
            "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n";
        let v4 = format!("{header}ens18\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n");
        let v4_local =
            format!("{header}ens18\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n");
        assert!(has_default_route(&v4, ""));
        assert!(!has_default_route(&v4_local, ""));
        let v6 = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000400 00000001 00000000 00000003 ens18\n";
        assert!(has_default_route("", v6));
    }

    #[test]
    fn writes_uptime_in_hours() {
        assert_eq!(
            format_uptime(Duration::from_secs(21 * 3600 + 12 * 60 + 59)),
            "21h 12m"
        );
        assert_eq!(format_uptime(Duration::from_secs(300)), "0h 05m");
    }
}
