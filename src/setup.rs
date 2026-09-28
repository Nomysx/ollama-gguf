// --add-to-path: make ollama-gguf a command in every new cmd or powershell window

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;

use anyhow::{Context, Result};
use windows_sys::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ};
use winreg::{RegKey, RegValue};

use crate::ui;

// copy the exe into the user programs folder and add that folder to the user path
pub fn add_to_path() -> Result<()> {
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from).context("LOCALAPPDATA is not set")?;
    let dir = local.join("Programs").join("ollama-gguf");
    let target = dir.join("ollama-gguf.exe");

    fs::create_dir_all(&dir)?;
    let current = env::current_exe()?;
    if !same_file(&current, &target) {
        fs::copy(&current, &target).with_context(|| format!("could not copy the exe to {}", dir.display()))?;
    }

    if add_path_entry(&dir)? {
        broadcast_environment_change();
        ui::success(format!("Installed to {} and added it to PATH", dir.display()));
    } else {
        ui::success(format!("Updated {}, it was already on PATH", target.display()));
    }
    ui::note("open a new terminal window and run: ollama-gguf --help");
    Ok(())
}

// running the installed copy must not copy the exe onto itself
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

// append dir to hkcu\environment\path unless present, keeping the value type so %VAR% entries still expand
fn add_path_entry(dir: &Path) -> Result<bool> {
    let environment = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .context("could not open the user environment")?;
    let current: String = environment.get_value("Path").unwrap_or_default();
    let vtype = environment.get_raw_value("Path").map_or(REG_EXPAND_SZ, |value| value.vtype);

    let Some(updated) = with_entry(&current, &dir.to_string_lossy()) else {
        return Ok(false);
    };
    let bytes: Vec<u8> = updated.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    environment
        .set_raw_value("Path", &RegValue { bytes: bytes.into(), vtype })
        .context("could not update PATH")?;
    Ok(true)
}

// the path value with dir appended, or none when dir is already listed
fn with_entry(path: &str, dir: &str) -> Option<String> {
    let same = |entry: &str| entry.trim().trim_end_matches('\\').eq_ignore_ascii_case(dir.trim_end_matches('\\'));
    if path.split(';').any(same) {
        return None;
    }
    let path = path.trim_end_matches(';');
    Some(if path.is_empty() { dir.to_string() } else { format!("{path};{dir}") })
}

// tell explorer the environment changed, so terminals opened from now on see the new path
fn broadcast_environment_change() {
    let area: Vec<u16> = "Environment\0".encode_utf16().collect();
    // safety: area is a nul terminated utf-16 string that outlives the call
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            ptr::null_mut(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::with_entry;

    #[test]
    fn appends_dir_once() {
        assert_eq!(with_entry(r"C:\a;C:\b", r"C:\tool").as_deref(), Some(r"C:\a;C:\b;C:\tool"));
        assert_eq!(with_entry(r"%USERPROFILE%\bin;", r"C:\tool").as_deref(), Some(r"%USERPROFILE%\bin;C:\tool"));
        assert_eq!(with_entry("", r"C:\tool").as_deref(), Some(r"C:\tool"));
        assert_eq!(with_entry(r"C:\a;c:\TOOL\;", r"C:\tool"), None);
    }
}
