//! Automatic Windows runtime prerequisite checker and silent installer.
//!
//! Runs automatically on startup (in <5 ms when everything is already installed),
//! ensures default configuration exists, verifies Windows 10/11 + Direct3D 11 /
//! Direct2D / DirectWrite / WIC / DWM runtimes and the Microsoft VC++ x64
//! Redistributable, silently installs missing redistributables via `winget`
//! (with official Microsoft download fallback), and writes a diagnostic log to
//! `%APPDATA%\winscroller-alt-tab\prereqs.log`.

use crate::config::Config;
use crate::win_platform;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::Graphics::Dwm::DwmIsCompositionEnabled;
use windows::Win32::System::LibraryLoader::LoadLibraryW;
use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MIN_WINDOWS_BUILD: u32 = 17763; // Windows 10 1809+
const VC_REDIST_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Installed,
    Warning,
    Missing,
}

#[derive(Clone, Debug)]
pub struct PrereqItem {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
    pub critical: bool,
}

#[derive(Clone, Debug)]
pub struct PrereqReport {
    pub items: Vec<PrereqItem>,
}

impl PrereqReport {
    pub fn all_critical_ok(&self) -> bool {
        self.items
            .iter()
            .filter(|i| i.critical)
            .all(|i| matches!(i.status, CheckStatus::Ok | CheckStatus::Installed))
    }

    pub fn has_any_missing(&self) -> bool {
        self.items
            .iter()
            .any(|i| matches!(i.status, CheckStatus::Missing))
    }

    pub fn format_summary(&self) -> String {
        let mut out = String::new();
        for item in &self.items {
            let tag = match item.status {
                CheckStatus::Ok => "[OK]       ",
                CheckStatus::Installed => "[INSTALLED]",
                CheckStatus::Warning => "[WARN]     ",
                CheckStatus::Missing => "[MISSING]  ",
            };
            out.push_str(&format!("{tag} {:<30} {}\n", item.name, item.detail));
        }
        out
    }
}

pub fn prereq_log_path() -> PathBuf {
    Config::config_path()
        .and_then(|p| p.parent().map(|d| d.join("prereqs.log")))
        .unwrap_or_else(|| PathBuf::from("winscroller-alt-tab-prereqs.log"))
}

/// Checks all runtime prerequisites. When `auto_install` is true, automatically
/// creates missing default configuration and silently installs missing
/// redistributable components (e.g. Microsoft VC++ 2015–2022 x64 Redistributable).
pub fn verify_and_install(
    config_override: Option<&Path>,
    auto_install: bool,
    interactive_alert: bool,
) -> PrereqReport {
    let mut items = Vec::new();
    let mut actions_taken = false;

    // 1. Windows 10 / 11 OS build check
    let build = read_windows_build_number();
    items.push(evaluate_windows_build(build));

    // 2. Native Direct3D 11 / Direct2D / DirectWrite / DXGI / WIC / DWM DLLs
    for &(label, dll) in &[
        ("Direct3D 11 Runtime", "d3d11.dll"),
        ("Direct2D 1.1 Runtime", "d2d1.dll"),
        ("DirectWrite Runtime", "dwrite.dll"),
        ("DXGI Graphics Infra", "dxgi.dll"),
        ("Windows Imaging (WIC)", "windowscodecs.dll"),
        ("Desktop Window Manager", "dwmapi.dll"),
    ] {
        let present = can_load_system_dll(dll);
        items.push(PrereqItem {
            name: label,
            status: if present {
                CheckStatus::Ok
            } else {
                CheckStatus::Missing
            },
            detail: if present {
                format!("{dll} loaded")
            } else {
                format!("{dll} not found in system PATH")
            },
            critical: true,
        });
    }

    // 3. DWM Composition status
    let dwm_on = is_dwm_composition_enabled();
    items.push(PrereqItem {
        name: "DWM Composition",
        status: if dwm_on {
            CheckStatus::Ok
        } else {
            CheckStatus::Warning
        },
        detail: if dwm_on {
            "Hardware desktop composition active".to_string()
        } else {
            "DWM composition disabled (live previews may be limited)".to_string()
        },
        critical: false,
    });

    // 4. Microsoft Visual C++ 2015–2022 Redistributable (x64)
    let vc_ok = is_vc_redist_x64_installed();
    if !vc_ok && auto_install {
        actions_taken = true;
        if install_vc_redist_x64() && is_vc_redist_x64_installed() {
            items.push(PrereqItem {
                name: "VC++ 2015-2022 x64 Runtime",
                status: CheckStatus::Installed,
                detail: "Automatically installed Microsoft VC++ x64 Redistributable".to_string(),
                critical: false,
            });
        } else {
            items.push(PrereqItem {
                name: "VC++ 2015-2022 x64 Runtime",
                status: CheckStatus::Warning,
                detail: "Auto-install could not complete (app still runs via static CRT)"
                    .to_string(),
                critical: false,
            });
        }
    } else {
        items.push(PrereqItem {
            name: "VC++ 2015-2022 x64 Runtime",
            status: if vc_ok {
                CheckStatus::Ok
            } else {
                CheckStatus::Missing
            },
            detail: if vc_ok {
                "Installed (vcruntime140.dll available)".to_string()
            } else {
                "Not detected (run with --install-prereqs to install automatically)".to_string()
            },
            critical: false,
        });
    }

    // 5. Default configuration file & AppData directory
    let cfg_path = config_override
        .map(Path::to_path_buf)
        .or_else(Config::config_path);
    match cfg_path {
        Some(path) if path.exists() => {
            items.push(PrereqItem {
                name: "User Configuration",
                status: CheckStatus::Ok,
                detail: format!("{}", path.display()),
                critical: false,
            });
        }
        Some(path) if auto_install => {
            actions_taken = true;
            if let Some(created) = Config::ensure_default_file_at(Some(&path)) {
                items.push(PrereqItem {
                    name: "User Configuration",
                    status: CheckStatus::Installed,
                    detail: format!("Initialized default config at {}", created.display()),
                    critical: false,
                });
            } else {
                items.push(PrereqItem {
                    name: "User Configuration",
                    status: CheckStatus::Warning,
                    detail: "Could not initialize default config.toml; using built-in defaults"
                        .to_string(),
                    critical: false,
                });
            }
        }
        Some(path) => {
            items.push(PrereqItem {
                name: "User Configuration",
                status: CheckStatus::Warning,
                detail: format!(
                    "Not yet created at {} (created automatically on launch)",
                    path.display()
                ),
                critical: false,
            });
        }
        None => {
            items.push(PrereqItem {
                name: "User Configuration",
                status: CheckStatus::Warning,
                detail: "%APPDATA% not set; using built-in defaults".to_string(),
                critical: false,
            });
        }
    }

    let report = PrereqReport { items };

    if actions_taken || !report.all_critical_ok() {
        write_prereq_log(&report);
    }

    if interactive_alert && !report.all_critical_ok() {
        show_missing_prereqs_dialog(&report);
    }

    report
}

fn read_windows_build_number() -> Option<u32> {
    win_platform::reg_read_sz(
        HKEY_LOCAL_MACHINE,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
        "CurrentBuildNumber",
    )
    .and_then(|s| s.trim().parse::<u32>().ok())
}

fn evaluate_windows_build(build: Option<u32>) -> PrereqItem {
    match build {
        Some(b) if b >= 22000 => PrereqItem {
            name: "Operating System",
            status: CheckStatus::Ok,
            detail: format!("Windows 11 (Build {b})"),
            critical: true,
        },
        Some(b) if b >= MIN_WINDOWS_BUILD => PrereqItem {
            name: "Operating System",
            status: CheckStatus::Ok,
            detail: format!("Windows 10 (Build {b})"),
            critical: true,
        },
        Some(b) => PrereqItem {
            name: "Operating System",
            status: CheckStatus::Missing,
            detail: format!(
                "Windows Build {b} is below minimum supported build {MIN_WINDOWS_BUILD}"
            ),
            critical: true,
        },
        None => PrereqItem {
            name: "Operating System",
            status: CheckStatus::Warning,
            detail: "Could not read Windows CurrentBuildNumber from registry".to_string(),
            critical: false,
        },
    }
}

fn can_load_system_dll(dll_name: &str) -> bool {
    let wide: Vec<u16> = dll_name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if let Ok(module) = LoadLibraryW(PCWSTR(wide.as_ptr())) {
            let _ = FreeLibrary(module);
            true
        } else {
            false
        }
    }
}

fn is_dwm_composition_enabled() -> bool {
    unsafe {
        DwmIsCompositionEnabled()
            .map(|b| b.as_bool())
            .unwrap_or(false)
    }
}

fn is_vc_redist_x64_installed() -> bool {
    let reg_installed = win_platform::reg_read_dword(
        HKEY_LOCAL_MACHINE,
        "SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64",
        "Installed",
    )
    .or_else(|| {
        win_platform::reg_read_dword(
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\WOW6432Node\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64",
            "Installed",
        )
    })
    .unwrap_or(0)
        == 1;

    reg_installed || can_load_system_dll("vcruntime140.dll")
}

/// Silently installs Microsoft Visual C++ 2015–2022 Redistributable (x64):
/// 1. Tries `winget` silently (zero external download logic needed on modern Windows 10/11).
/// 2. Falls back to downloading Microsoft's official `vc_redist.x64.exe` and running `/install /quiet /norestart`.
fn install_vc_redist_x64() -> bool {
    // Strategy 1: Windows Package Manager (winget)
    if let Ok(status) = Command::new("winget")
        .args([
            "install",
            "--id",
            "Microsoft.VCRedist.2015+.x64",
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        && status.success()
    {
        return true;
    }

    // Strategy 2: Official Microsoft vc_redist.x64.exe silent installer
    let installer = std::env::temp_dir().join("winscroller_vc_redist.x64.exe");
    let dl_script = format!(
        "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         Invoke-WebRequest -UseBasicParsing -Uri '{VC_REDIST_URL}' -OutFile '{}'",
        installer.display()
    );
    let downloaded = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &dl_script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success())
        && installer.exists();

    if !downloaded {
        return false;
    }

    let ok = Command::new(&installer)
        .args(["/install", "/quiet", "/norestart"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success() || s.code() == Some(3010)); // 3010 = success, reboot optional

    let _ = fs::remove_file(&installer);
    ok
}

fn write_prereq_log(report: &PrereqReport) {
    let path = prereq_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format!(
        "[unix:{ts}] Winscroller Alt+Tab prerequisite check:\n{}\n",
        report.format_summary()
    );
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(body.as_bytes()));
}

fn show_missing_prereqs_dialog(report: &PrereqReport) {
    let log = prereq_log_path();
    let text = format!(
        "Winscroller Alt+Tab detected missing critical Windows runtime prerequisites:\n\n{}\nDetails logged to:\n{}",
        report.format_summary(),
        log.display()
    );
    let title = "Winscroller Alt+Tab — Missing Prerequisites";
    let w_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let w_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(w_text.as_ptr()),
            PCWSTR(w_title.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_windows_builds_accurately() {
        let w11 = evaluate_windows_build(Some(26100));
        assert_eq!(w11.status, CheckStatus::Ok);
        assert!(w11.detail.contains("Windows 11"));

        let w10 = evaluate_windows_build(Some(19045));
        assert_eq!(w10.status, CheckStatus::Ok);
        assert!(w10.detail.contains("Windows 10"));

        let old = evaluate_windows_build(Some(10240));
        assert_eq!(old.status, CheckStatus::Missing);
        assert!(old.critical);
    }

    #[test]
    fn report_detects_critical_failures() {
        let ok_report = PrereqReport {
            items: vec![
                PrereqItem {
                    name: "Direct3D 11",
                    status: CheckStatus::Ok,
                    detail: "loaded".into(),
                    critical: true,
                },
                PrereqItem {
                    name: "VC++",
                    status: CheckStatus::Installed,
                    detail: "installed".into(),
                    critical: false,
                },
            ],
        };
        assert!(ok_report.all_critical_ok());
        assert!(!ok_report.has_any_missing());

        let bad_report = PrereqReport {
            items: vec![PrereqItem {
                name: "Direct3D 11",
                status: CheckStatus::Missing,
                detail: "missing".into(),
                critical: true,
            }],
        };
        assert!(!bad_report.all_critical_ok());
        assert!(bad_report.has_any_missing());
    }
}
