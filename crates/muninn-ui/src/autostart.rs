//! Settings › Appearance › Start Muninn at login.
//!
//! Rune has no login-item API at the pinned revision, so Muninn writes the
//! OS's own entry with the standard library: an XDG autostart desktop file
//! (`$XDG_CONFIG_HOME/autostart/muninn.desktop`) on Linux, and a value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (through the system
//! `reg.exe`) on Windows. macOS is not supported yet.
//!
//! [`State`] reaches the OS through an [`AutostartPort`] the binary installs,
//! so tests never touch the real login items. Headless runs install none.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::State;

/// The autostart file name and the Windows Run value name.
pub const AUTOSTART_NAME: &str = "Muninn";

/// Whether Muninn can add a login item on this platform.
#[must_use]
pub const fn autostart_supported() -> bool {
    cfg!(any(target_os = "linux", target_os = "windows"))
}

/// Adds or removes Muninn's login item.
pub trait AutostartPort {
    /// Make the OS start Muninn at login (`true`) or stop doing so (`false`).
    /// Idempotent: enabling again re-points the entry at this executable.
    fn apply(&self, on: bool) -> Result<(), String>;
}

/// The login item as [`State`] holds it.
#[derive(Default)]
pub(crate) struct AutostartLink {
    port: Option<Box<dyn AutostartPort>>,
    /// Why the last change failed. Cleared when the switch changes.
    error: Option<String>,
}

impl State {
    /// Settings › Appearance › Start Muninn at login (the saved choice).
    #[must_use]
    pub fn auto_start(&self) -> bool {
        self.settings.auto_start
    }

    /// Turn Start at login on or off, persist the choice and update the OS
    /// entry now.
    pub fn set_auto_start(&mut self, on: bool) {
        self.settings.auto_start = on;
        self.autostart.error = None;
        self.save_settings();
        self.apply_autostart(on);
    }

    /// Install how this process changes its login item. With the setting on,
    /// the entry is rewritten so it points at this executable.
    pub fn install_autostart(&mut self, port: Box<dyn AutostartPort>) {
        self.autostart.port = Some(port);
        self.autostart.error = None;
        if self.settings.auto_start {
            self.apply_autostart(true);
        }
    }

    /// Why the login item could not be changed.
    #[must_use]
    pub fn auto_start_error(&self) -> Option<&str> {
        self.autostart.error.as_deref()
    }

    fn apply_autostart(&mut self, on: bool) {
        if !autostart_supported() {
            return;
        }
        let Some(port) = self.autostart.port.as_ref() else {
            return;
        };
        if let Err(error) = port.apply(on) {
            self.log_event("autostart-error", &error);
            self.autostart.error = Some(error);
        }
    }
}

/// The OS login item for this executable.
pub struct NativeAutostart {
    /// Linux: the XDG config root that holds `autostart/`.
    config_home: PathBuf,
}

impl NativeAutostart {
    /// The login item under the user's XDG config root.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config_home: crate::config_home(),
        }
    }
}

impl Default for NativeAutostart {
    fn default() -> Self {
        Self::new()
    }
}

impl AutostartPort for NativeAutostart {
    fn apply(&self, on: bool) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            let _ = &self.config_home;
            if on {
                let exe = launch_path().map_err(|e| e.to_string())?;
                windows_run(&windows_enable_args(&exe))
            } else {
                windows_disable()
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let dir = autostart_dir(&self.config_home);
            let result = if on {
                launch_path().and_then(|exe| write_desktop_entry(&dir, &exe))
            } else {
                remove_desktop_entry(&dir)
            };
            result.map_err(|e| e.to_string())
        }
    }
}

/// What the login item launches: the AppImage when running from one (its
/// mount point changes every run), else this executable.
fn launch_path() -> io::Result<PathBuf> {
    if let Some(image) = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(image));
    }
    std::env::current_exe()
}

/// `$XDG_CONFIG_HOME/autostart`.
#[must_use]
pub fn autostart_dir(config_home: &Path) -> PathBuf {
    config_home.join("autostart")
}

/// The autostart desktop file's path in `dir`.
#[must_use]
pub fn desktop_entry_path(dir: &Path) -> PathBuf {
    dir.join("muninn.desktop")
}

/// The XDG autostart entry that launches `exe`.
///
/// # Errors
/// A path with a line break or that is not UTF-8 cannot be written into a
/// desktop entry.
pub fn desktop_entry(exe: &Path) -> io::Result<String> {
    let exec = desktop_exec(exe)?;
    Ok(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={AUTOSTART_NAME}\n\
         Comment=Start Muninn at login (Settings › Appearance)\n\
         Exec={exec}\n\
         Icon=muninn\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    ))
}

/// `exe` quoted for a desktop entry's `Exec` key: inside double quotes,
/// `"`, `` ` ``, `$` and `\` are backslash-escaped, `%` is doubled, and then
/// the string-value escape doubles every backslash again.
fn desktop_exec(exe: &Path) -> io::Result<String> {
    let path = exe.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the executable path is not UTF-8",
        )
    })?;
    if path.contains(['\n', '\r']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the executable path has a line break",
        ));
    }
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    Ok(quoted.replace('\\', "\\\\"))
}

/// Write the autostart entry for `exe` into `dir` (atomically).
///
/// # Errors
/// The directory or file could not be written.
pub fn write_desktop_entry(dir: &Path, exe: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = desktop_entry_path(dir);
    let tmp = path.with_extension("desktop.tmp");
    fs::write(&tmp, desktop_entry(exe)?)?;
    fs::rename(&tmp, &path)
}

/// Remove the autostart entry from `dir`; a missing one is fine.
///
/// # Errors
/// The file exists and could not be removed.
pub fn remove_desktop_entry(dir: &Path) -> io::Result<()> {
    match fs::remove_file(desktop_entry_path(dir)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The Windows Run key Muninn writes under.
pub const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// `reg.exe` arguments that make Windows start `exe` at login.
#[must_use]
pub fn windows_enable_args(exe: &Path) -> Vec<String> {
    vec![
        "add".to_owned(),
        WINDOWS_RUN_KEY.to_owned(),
        "/v".to_owned(),
        AUTOSTART_NAME.to_owned(),
        "/t".to_owned(),
        "REG_SZ".to_owned(),
        "/d".to_owned(),
        format!("\"{}\"", exe.display()),
        "/f".to_owned(),
    ]
}

/// `reg.exe` arguments that remove Muninn's Run value.
#[must_use]
pub fn windows_disable_args() -> Vec<String> {
    vec![
        "delete".to_owned(),
        WINDOWS_RUN_KEY.to_owned(),
        "/v".to_owned(),
        AUTOSTART_NAME.to_owned(),
        "/f".to_owned(),
    ]
}

#[cfg(target_os = "windows")]
fn windows_run(args: &[String]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    /// No console window for `reg.exe`.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("reg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("reg.exe could not run: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if message.is_empty() {
            format!("reg.exe exited with {}", output.status)
        } else {
            message
        })
    }
}

/// Remove the Run value; one that is already gone is fine.
#[cfg(target_os = "windows")]
fn windows_disable() -> Result<(), String> {
    match windows_run(&windows_disable_args()) {
        Ok(()) => Ok(()),
        Err(error) => {
            let query = vec![
                "query".to_owned(),
                WINDOWS_RUN_KEY.to_owned(),
                "/v".to_owned(),
                AUTOSTART_NAME.to_owned(),
            ];
            if windows_run(&query).is_err() {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_entry_launches_the_quoted_executable() {
        let entry = desktop_entry(Path::new("/opt/Muninn App/muninn")).expect("entry");
        assert!(entry.starts_with("[Desktop Entry]\nType=Application\n"));
        assert!(entry.contains("\nExec=\"/opt/Muninn App/muninn\"\n"));
        assert!(entry.contains("\nName=Muninn\n"));
        assert!(entry.contains("\nX-GNOME-Autostart-enabled=true\n"));
    }

    #[test]
    fn exec_escapes_reserved_characters() {
        assert_eq!(
            desktop_exec(Path::new("/a$b/100%/q\"t/x`y")).expect("exec"),
            "\"/a\\\\$b/100%%/q\\\\\"t/x\\\\`y\""
        );
        assert_eq!(
            desktop_exec(Path::new("/back\\slash")).expect("exec"),
            "\"/back\\\\\\\\slash\""
        );
        assert!(desktop_exec(Path::new("/line\nbreak")).is_err());
    }

    #[test]
    fn windows_arguments_name_the_run_value() {
        let on = windows_enable_args(Path::new(r"C:\Program Files\Muninn\muninn.exe"));
        assert_eq!(on[0], "add");
        assert_eq!(on[1], WINDOWS_RUN_KEY);
        assert_eq!(&on[2..4], ["/v", "Muninn"]);
        assert_eq!(on[7], r#""C:\Program Files\Muninn\muninn.exe""#);
        assert_eq!(on.last().map(String::as_str), Some("/f"));
        assert_eq!(
            windows_disable_args(),
            ["delete", WINDOWS_RUN_KEY, "/v", "Muninn", "/f"]
        );
    }
}
