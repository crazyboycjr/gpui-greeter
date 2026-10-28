//! What a fetch block says about the machine: the distribution, the
//! processor and graphics cards, memory, packages and the user's shell,
//! read from files the greeter user may read, never by running anything.

use std::collections::HashMap;
use std::path::Path;

use super::template::Binding;

/// The facts a theme's text uses, so only those are read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Uses {
    pub cpu: bool,
    pub gpu: bool,
    pub memory: bool,
    pub packages: bool,
    pub shell: bool,
}

impl Uses {
    pub fn note(&mut self, binding: &Binding) {
        match binding {
            Binding::Cpu => self.cpu = true,
            Binding::Gpu => self.gpu = true,
            Binding::Memory => self.memory = true,
            Binding::Packages => self.packages = true,
            Binding::Shell => self.shell = true,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// "Arch Linux".
    pub os: String,
    /// "x86_64".
    pub arch: String,
    /// "AMD Ryzen 7 5800X (16)".
    pub cpu: String,
    /// "NVIDIA GeForce RTX 3060".
    pub gpu: String,
    /// "1234 (pacman)".
    pub packages: String,
    /// By the shell's path: "zsh 5.9".
    pub shells: HashMap<String, String>,
}

impl Facts {
    /// Reads what `uses` asks for, and the shells of `shells`.
    pub fn read<'a>(uses: Uses, shells: impl IntoIterator<Item = &'a str>) -> Self {
        let file = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let arch = std::fs::read_to_string("/proc/sys/kernel/arch")
            .map(|arch| arch.trim().to_string())
            .unwrap_or_else(|_| std::env::consts::ARCH.to_string());
        let mut facts = Facts {
            os: os_name(&file("/etc/os-release")),
            arch,
            ..Facts::default()
        };
        if uses.cpu {
            facts.cpu = cpu(&file("/proc/cpuinfo"));
        }
        if uses.gpu {
            facts.gpu = gpus();
        }
        if uses.packages {
            facts.packages = packages(Path::new("/"));
        }
        if uses.shell {
            for shell in shells {
                facts
                    .shells
                    .entry(shell.to_string())
                    .or_insert_with(|| shell_name(Path::new("/"), shell));
            }
        }
        facts
    }
}

/// The distribution's own name for itself, "Arch Linux".
pub fn os_name(os_release: &str) -> String {
    let value = |key: &str| {
        os_release.lines().find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|value| value.trim().trim_matches('"').to_string())
        })
    };
    value("PRETTY_NAME")
        .or_else(|| value("NAME"))
        .unwrap_or_else(|| "Linux".into())
}

/// "AMD Ryzen 7 5800X (16)": the model without its marketing tail, and
/// the number of logical processors.
pub fn cpu(cpuinfo: &str) -> String {
    let model = cpuinfo.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        matches!(
            key.trim(),
            "model name" | "Model" | "Hardware" | "cpu model"
        )
        .then(|| value.trim().to_string())
    });
    let count = cpuinfo
        .lines()
        .filter(|line| {
            line.split(':')
                .next()
                .is_some_and(|key| key.trim() == "processor")
        })
        .count();
    let Some(mut model) = model else {
        return String::new();
    };
    if let Some(at) = model.find(" @ ") {
        model.truncate(at);
    }
    for tail in [" CPU", " Processor"] {
        if let Some(stripped) = model.strip_suffix(tail) {
            model = stripped.to_string();
        }
    }
    if let Some((rest, cores)) = model.rsplit_once(' ')
        && cores.ends_with("-Core")
    {
        model = rest.to_string();
    }
    for graphics in [" with Radeon Graphics", " w/ Radeon Graphics"] {
        if let Some(at) = model.find(graphics) {
            model.truncate(at);
        }
    }
    match count {
        0 => model,
        count => format!("{model} ({count})"),
    }
}

/// The graphics cards, by the names their drivers or the PCI database give.
pub fn gpus() -> String {
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc/driver/nvidia/gpus") {
        for entry in entries.flatten() {
            let information =
                std::fs::read_to_string(entry.path().join("information")).unwrap_or_default();
            if let Some(model) = information
                .lines()
                .find_map(|line| line.strip_prefix("Model:"))
            {
                names.push(model.trim().to_string());
            }
        }
    }
    if names.is_empty() {
        let database = ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"]
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let mut cards: Vec<_> = std::fs::read_dir("/sys/class/drm")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.strip_prefix("card")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
            .map(|entry| entry.path().join("device"))
            .collect();
        cards.sort();
        for device in cards {
            let id = |file: &str| {
                std::fs::read_to_string(device.join(file))
                    .ok()
                    .and_then(|text| {
                        u16::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
                    })
            };
            if let (Some(vendor), Some(model)) = (id("vendor"), id("device"))
                && let Some(name) = pci_name(&database, vendor, model)
            {
                names.push(name);
            }
        }
    }
    names.dedup();
    names.join(", ")
}

/// A card's name from the PCI database: the vendor's short name and the
/// model, "NVIDIA GeForce RTX 3060" for `GA106 [GeForce RTX
/// 3060]`.
pub fn pci_name(database: &str, vendor: u16, device: u16) -> Option<String> {
    let vendor_id = format!("{vendor:04x}");
    let device_id = format!("{device:04x}");
    let mut lines = database.lines().skip_while(|line| {
        !(line.starts_with(&vendor_id) && line.as_bytes().get(4) == Some(&b' '))
    });
    let vendor_name = lines.next()?[4..].trim().to_string();
    let model = lines
        .take_while(|line| line.starts_with('\t') || line.starts_with('#') || line.is_empty())
        .find_map(|line| {
            let line = line.strip_prefix('\t')?;
            (!line.starts_with('\t') && line.starts_with(&device_id))
                .then(|| line[4..].trim().to_string())
        })?;
    let model = match (model.find('['), model.rfind(']')) {
        (Some(open), Some(close)) if open < close => model[open + 1..close].to_string(),
        _ => model,
    };
    let short = match vendor_name.as_str() {
        name if name.starts_with("Advanced Micro Devices") => "AMD".to_string(),
        name => name
            .split(" [")
            .next()
            .unwrap_or(name)
            .trim_end_matches(" Corporation")
            .trim_end_matches(", Inc.")
            .trim_end_matches(" Inc.")
            .to_string(),
    };
    Some(if model.starts_with(&short) {
        model
    } else {
        format!("{short} {model}")
    })
}

/// "3.81 GiB / 15.26 GiB": what is in use, of all there is.
pub fn memory(meminfo: &str) -> String {
    let kib = |key: &str| {
        meminfo.lines().find_map(|line| {
            line.strip_prefix(key)?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok()
        })
    };
    let (Some(total), Some(available)) = (kib("MemTotal:"), kib("MemAvailable:")) else {
        return String::new();
    };
    let gib = |kib: u64| kib as f64 / (1024.0 * 1024.0);
    format!(
        "{:.2} GiB / {:.2} GiB",
        gib(total.saturating_sub(available)),
        gib(total)
    )
}

/// "1234 (pacman), 12 (flatpak)": what each package manager has installed.
pub fn packages(root: &Path) -> String {
    let count_dirs = |path: &Path| {
        std::fs::read_dir(path).map_or(0, |entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .count()
        })
    };
    let dpkg = std::fs::read_to_string(root.join("var/lib/dpkg/status")).map_or(0, |status| {
        status.matches("Status: install ok installed").count()
    });
    [
        (count_dirs(&root.join("var/lib/pacman/local")), "pacman"),
        (dpkg, "dpkg"),
        (count_dirs(&root.join("var/lib/flatpak/app")), "flatpak"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, manager)| format!("{count} ({manager})"))
    .collect::<Vec<_>>()
    .join(", ")
}

/// "zsh 5.9": the shell's name, and its version when the package manager
/// keeps a package of that name.
pub fn shell_name(root: &Path, shell: &str) -> String {
    let name = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    if name.is_empty() {
        return name;
    }
    let pacman = std::fs::read_dir(root.join("var/lib/pacman/local"))
        .into_iter()
        .flatten()
        .flatten()
        .find_map(|entry| {
            let dir = entry.file_name().to_string_lossy().into_owned();
            let mut parts = dir.rsplitn(3, '-');
            let (_release, version, package) = (parts.next()?, parts.next()?, parts.next()?);
            (package == name).then(|| version.to_string())
        });
    let dpkg = || {
        let status = std::fs::read_to_string(root.join("var/lib/dpkg/status")).ok()?;
        status.split("\n\n").find_map(|stanza| {
            stanza
                .lines()
                .any(|line| line == format!("Package: {name}"))
                .then(|| {
                    stanza
                        .lines()
                        .find_map(|line| line.strip_prefix("Version: "))
                        .map(|version| version.split('-').next().unwrap_or(version).to_string())
                })
                .flatten()
        })
    };
    let version = pacman
        .or_else(dpkg)
        .map(|version| match version.split_once(':') {
            Some((_epoch, version)) => version.to_string(),
            None => version,
        });
    match version {
        Some(version) => format!("{name} {version}"),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_distribution() {
        assert_eq!(
            os_name("NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n"),
            "Arch Linux"
        );
        assert_eq!(os_name("NAME=Debian\n"), "Debian");
        assert_eq!(os_name(""), "Linux");
    }

    #[test]
    fn names_the_processor_as_fetch_does() {
        let amd = "processor\t: 0\nmodel name\t: AMD Ryzen 7 5800X 8-Core Processor\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7 5800X 8-Core Processor\n";
        assert_eq!(cpu(amd), "AMD Ryzen 7 5800X (2)");
        let intel = "processor\t: 0\nmodel name\t: Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz\n";
        assert_eq!(cpu(intel), "Intel(R) Core(TM) i7-8700K (1)");
        assert_eq!(cpu(""), "");
    }

    #[test]
    fn names_cards_from_the_pci_database() {
        let database = "# comment\n10de  NVIDIA Corporation\n\t2503  GA106 [GeForce RTX 3060]\n\t\t1458 4071  Subsystem\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t744c  Navi 31 [Radeon RX 7900 XT/7900 XTX]\n1af4  Red Hat, Inc.\n\t1050  Virtio 1.0 GPU\n";
        assert_eq!(
            pci_name(database, 0x10de, 0x2503).as_deref(),
            Some("NVIDIA GeForce RTX 3060")
        );
        assert_eq!(
            pci_name(database, 0x1002, 0x744c).as_deref(),
            Some("AMD Radeon RX 7900 XT/7900 XTX")
        );
        assert_eq!(
            pci_name(database, 0x1af4, 0x1050).as_deref(),
            Some("Red Hat Virtio 1.0 GPU")
        );
        assert_eq!(pci_name(database, 0x10de, 0x1111), None);
    }

    #[test]
    fn reads_memory_in_use() {
        let meminfo = "MemTotal:       16000000 kB\nMemFree:  1 kB\nMemAvailable:   12000000 kB\n";
        assert_eq!(memory(meminfo), "3.81 GiB / 15.26 GiB");
        assert_eq!(memory(""), "");
    }

    #[test]
    fn counts_packages_and_finds_the_shell() {
        let root = std::env::temp_dir().join(format!("gpui-greeter-facts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for package in ["zsh-5.9-5", "zsh-completions-0.35-1", "bash-5.2.037-5"] {
            std::fs::create_dir_all(root.join("var/lib/pacman/local").join(package)).unwrap();
        }
        std::fs::write(root.join("var/lib/pacman/local/ALPM_DB_VERSION"), "9").unwrap();
        assert_eq!(packages(&root), "3 (pacman)");
        assert_eq!(shell_name(&root, "/usr/bin/zsh"), "zsh 5.9");
        assert_eq!(shell_name(&root, "/bin/fish"), "fish");
        let _ = std::fs::remove_dir_all(root);
    }
}
