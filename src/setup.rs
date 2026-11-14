//! First-run guidance: one command that takes you from "downloaded a file" to "protected".
//!
//! Setup fails, and it usually fails for boring reasons: the data directory is world-readable,
//! the platform has no keystroke capture, the clipboard tool is missing, you forgot to start it
//! at login. Each of those is a five-second fix that a user will not discover alone, so they are
//! checked here and reported with the exact command that fixes them.
//!
//! Two rules:
//!
//! - **Never claim more than is true.** On Linux there is no keystroke capture; setup says so in
//!   the first ten lines instead of letting the user find out during a crash.
//! - **Every line is actionable.** A check that cannot tell the user what to do about it is
//!   noise, so checks return a [`Step`] with `fix: Option<String>` rather than a bare boolean.

use std::path::Path;

/// The outcome of one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: &'static str,
    /// `Ok`, `Warn` or `Fail`.
    pub level: Level,
    pub detail: String,
    /// A command or action the user can run, when there is one.
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

impl Level {
    pub fn mark(&self) -> &'static str {
        match self {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        }
    }
}

impl Step {
    fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Self { name, level: Level::Ok, detail: detail.into(), fix: None }
    }
    fn warn(name: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Self { name, level: Level::Warn, detail: detail.into(), fix: Some(fix.into()) }
    }
    #[cfg(unix)]
    fn fail(name: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Self { name, level: Level::Fail, detail: detail.into(), fix: Some(fix.into()) }
    }
}

/// Where the capture actually works on this platform. Stated, not inferred by the user.
pub fn capture_claim() -> (&'static str, &'static str) {
    #[cfg(target_os = "windows")]
    {
        ("keystrokes, system-wide", "uses the low-level keyboard hook; no elevation needed")
    }
    #[cfg(target_os = "macos")]
    {
        (
            "keystrokes, system-wide",
            "needs Accessibility permission (System Settings → Privacy & Security)",
        )
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ("the clipboard", "Linux has no safe system-wide keystroke API; this is a deliberate limit")
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        ("nothing yet", "unsupported platform")
    }
}

/// Runs every check that can be run without capturing anything.
pub fn checks(data_dir: &Path) -> Vec<Step> {
    let mut steps = Vec::new();

    // 1. Platform capability, stated honestly and first.
    let (what, why) = capture_claim();
    let capture = Step {
        name: "capture",
        level: if what == "nothing yet" { Level::Fail } else { Level::Ok },
        detail: format!("{what} — {why}"),
        fix: if what == "nothing yet" {
            Some("tell us your platform: https://github.com/AMG555/Tether/issues".into())
        } else {
            None
        },
    };
    steps.push(capture);

    // 2. Data directory exists and is private.
    if data_dir.exists() {
        steps.push(Step::ok("data dir", data_dir.display().to_string()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(m) = std::fs::metadata(data_dir) {
                let mode = m.permissions().mode() & 0o777;
                if mode & 0o077 == 0 {
                    steps.push(Step::ok("permissions", format!("{mode:o} (private)")));
                } else {
                    steps.push(Step::fail(
                        "permissions",
                        format!("{mode:o} — other users can read this directory"),
                        format!("chmod 700 {}", data_dir.display()),
                    ));
                }
            }
        }
        #[cfg(not(unix))]
        steps.push(Step::ok("permissions", "inherits your user-profile ACL"));
    } else {
        steps.push(Step::warn(
            "data dir",
            format!("{} does not exist yet", data_dir.display()),
            "tether init",
        ));
    }

    // 3. The key exists (a vault without a key is unreadable, forever).
    let key = data_dir.join("key.txt");
    if key.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                std::fs::metadata(&key).map(|m| m.permissions().mode() & 0o777).unwrap_or(0o644);
            if mode & 0o077 == 0 {
                steps.push(Step::ok("key file", "present, private"));
            } else {
                steps.push(Step::fail(
                    "key file",
                    format!("mode {mode:o} — anyone on this machine can decrypt your vault"),
                    format!("chmod 600 {}", key.display()),
                ));
            }
        }
        #[cfg(not(unix))]
        steps.push(Step::ok("key file", "present"));
        steps.push(Step::warn(
            "key backup",
            "the key sits next to the vault: losing the disk loses both",
            format!("copy {} somewhere separate (password manager, USB stick)", key.display()),
        ));
    } else {
        steps.push(Step::warn("key file", "no vault yet", "tether init"));
    }

    // 4. Start at login: the whole point is that it is already running when the crash happens.
    steps.push(autostart_step());

    // 5. The companion is optional; say so rather than implying it is needed.
    steps.push(Step::ok(
        "companion",
        "optional — `tether ask --new NAME` makes a pet, `tether ask --character-studio` "
            .to_string()
            + "authors a realistic one",
    ));

    steps
}

#[cfg(target_os = "linux")]
fn autostart_step() -> Step {
    let dir = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(v) => std::path::PathBuf::from(v),
        None => match std::env::var_os("HOME") {
            Some(h) => std::path::PathBuf::from(h).join(".config"),
            None => {
                return Step::warn(
                    "start at login",
                    "cannot find your config directory",
                    "tether setup --autostart",
                )
            }
        },
    };
    let unit = dir.join("systemd/user/tether.service");
    if unit.exists() {
        Step::ok("start at login", unit.display().to_string())
    } else {
        Step::warn(
            "start at login",
            "not installed — a crash only counts if Tether was already running",
            "tether setup --autostart",
        )
    }
}

#[cfg(target_os = "macos")]
fn autostart_step() -> Step {
    match std::env::var_os("HOME") {
        Some(h) => {
            let plist =
                std::path::PathBuf::from(h).join("Library/LaunchAgents/dev.tether.agent.plist");
            if plist.exists() {
                Step::ok("start at login", plist.display().to_string())
            } else {
                Step::warn(
                    "start at login",
                    "not installed — a crash only counts if Tether was already running",
                    "tether setup --autostart",
                )
            }
        }
        None => Step::warn(
            "start at login",
            "cannot find your home directory",
            "tether setup --autostart",
        ),
    }
}

#[cfg(target_os = "windows")]
fn autostart_step() -> Step {
    // %APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup is the no-admin option.
    match std::env::var_os("APPDATA") {
        Some(a) => {
            let lnk = std::path::PathBuf::from(a)
                .join("Microsoft/Windows/Start Menu/Programs/Startup/tether.url");
            if lnk.exists() {
                Step::ok("start at login", "Startup folder entry present")
            } else {
                Step::warn(
                    "start at login",
                    "not installed — a crash only counts if Tether was already running",
                    "tether setup --autostart",
                )
            }
        }
        None => Step::warn("start at login", "cannot find %APPDATA%", "tether setup --autostart"),
    }
}

/// Writes a login item for this platform. Returns the path written.
///
/// Deliberately uses each OS's *user-level* mechanism: a systemd user unit, a LaunchAgent, a
/// Startup-folder shortcut. All three are removable by the user without an administrator.
pub fn install_autostart(exe: &Path) -> Result<std::path::PathBuf, String> {
    let exe = exe.display().to_string();

    #[cfg(target_os = "linux")]
    {
        let dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
            })
            .ok_or("cannot find your config directory")?
            .join("systemd/user");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("tether.service");
        let unit = format!(
            "[Unit]\n\
             Description=Tether — crash-proof typing recovery\n\
             After=graphical-session.target\n\n\
             [Service]\n\
             ExecStart={exe} run\n\
             Restart=always\n\
             RestartSec=3\n\
             # Capture stays in your session; nothing here talks to the network.\n\n\
             [Install]\n\
             WantedBy=default.target\n"
        );
        std::fs::write(&path, unit).map_err(|e| e.to_string())?;
        Ok(path)
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("cannot find your home directory")?;
        let dir = std::path::PathBuf::from(home).join("Library/LaunchAgents");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("dev.tether.agent.plist");
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>dev.tether.agent</string>
  <key>ProgramArguments</key>
  <array><string>{exe}</string><string>run</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
        );
        std::fs::write(&path, plist).map_err(|e| e.to_string())?;
        Ok(path)
    }

    #[cfg(target_os = "windows")]
    {
        // A .url shortcut in the Startup folder needs no script host and no admin rights.
        let appdata = std::env::var_os("APPDATA").ok_or("cannot find %APPDATA%")?;
        let dir =
            std::path::PathBuf::from(appdata).join("Microsoft/Windows/Start Menu/Programs/Startup");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("tether.url");
        // `run` in a console window is not ideal on Windows; point at the same command.
        std::fs::write(&path, format!("[InternetShortcut]\nURL=file:///{exe}\n"))
            .map_err(|e| e.to_string())?;
        Ok(path)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = exe;
        Err("autostart is not implemented for this platform".into())
    }
}

/// Remove user-level login/startup items installed by `install_autostart`.
pub fn uninstall_autostart() -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        let mut removed = false;
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
            })
        {
            let p1 = dir.join("systemd/user/tether.service");
            if p1.exists() {
                let _ = std::fs::remove_file(p1);
                removed = true;
            }
            let p2 = dir.join("autostart/tether.desktop");
            if p2.exists() {
                let _ = std::fs::remove_file(p2);
                removed = true;
            }
        }
        Ok(removed)
    }

    #[cfg(target_os = "macos")]
    {
        let mut removed = false;
        if let Some(home) = std::env::var_os("HOME") {
            let dir = std::path::PathBuf::from(home).join("Library/LaunchAgents");
            let p1 = dir.join("dev.tether.agent.plist");
            if p1.exists() {
                let _ = std::fs::remove_file(p1);
                removed = true;
            }
            let p2 = dir.join("dev.tether.plist");
            if p2.exists() {
                let _ = std::fs::remove_file(p2);
                removed = true;
            }
        }
        Ok(removed)
    }

    #[cfg(target_os = "windows")]
    {
        let mut removed = false;
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let dir = std::path::PathBuf::from(appdata)
                .join("Microsoft/Windows/Start Menu/Programs/Startup");
            let p1 = dir.join("tether.url");
            if p1.exists() {
                let _ = std::fs::remove_file(p1);
                removed = true;
            }
            let p2 = dir.join("tether.lnk");
            if p2.exists() {
                let _ = std::fs::remove_file(p2);
                removed = true;
            }
        }
        Ok(removed)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err("autostart is not implemented for this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_claim_is_honest_on_this_platform() {
        let (what, why) = capture_claim();
        assert!(!what.is_empty() && !why.is_empty());
        #[cfg(target_os = "linux")]
        {
            // The Linux limitation must be stated, not hidden.
            assert_eq!(what, "the clipboard");
            assert!(why.contains("deliberate"));
        }
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        assert!(what.contains("keystroke"), "got {what}");
    }

    #[test]
    fn every_warning_and_failure_tells_the_user_what_to_do() {
        let d = tempfile::tempdir().unwrap();
        for step in checks(d.path()) {
            if step.level != Level::Ok {
                assert!(
                    step.fix.is_some(),
                    "check {:?} complains without a fix: {}",
                    step.name,
                    step.detail
                );
            }
            assert!(!step.detail.contains("TODO"), "check {:?} has unfinished text", step.name);
        }
    }

    #[test]
    fn an_uninitialised_directory_is_reported_as_such() {
        let d = tempfile::tempdir().unwrap();
        let missing = d.path().join("nope");
        let steps = checks(&missing);
        let dir = steps.iter().find(|s| s.name == "data dir").unwrap();
        assert_eq!(dir.level, Level::Warn);
        assert_eq!(dir.fix.as_deref(), Some("tether init"));
    }

    #[test]
    fn a_world_readable_key_file_is_a_failure_with_the_chmod_command() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let d = tempfile::tempdir().unwrap();
            let key = d.path().join("key.txt");
            std::fs::write(&key, "00").unwrap();
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
            let steps = checks(d.path());
            let step = steps.iter().find(|s| s.name == "key file").unwrap();
            assert_eq!(step.level, Level::Fail);
            assert!(step.fix.as_ref().unwrap().starts_with("chmod 600"), "{:?}", step.fix);
        }
    }

    #[test]
    fn autostart_writes_a_file_naming_the_binary() {
        let d = tempfile::tempdir().unwrap();
        // Redirect the location the unit is written to.
        #[cfg(target_os = "linux")]
        std::env::set_var("XDG_CONFIG_HOME", d.path());
        #[cfg(target_os = "macos")]
        std::env::set_var("HOME", d.path());

        match install_autostart(Path::new("/usr/local/bin/tether")) {
            Ok(path) => {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(text.contains("tether"), "the unit must name the binary");
                assert!(path.starts_with(d.path()) || path.exists());
            }
            Err(e) => panic!("autostart failed: {e}"),
        }
    }
}
