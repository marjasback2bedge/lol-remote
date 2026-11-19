//! Start on login (spec §10): a LaunchAgent on macOS, a Run key on Windows.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};

const LABEL: &str = "dev.lol-remote";

fn log_dir() -> anyhow::Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "lol-remote", "lol-remote")
        .context("could not determine a data directory")?;
    Ok(dirs.data_local_dir().join("logs"))
}

fn run(cmd: &mut Command) -> anyhow::Result<std::process::Output> {
    let out = cmd.output().with_context(|| format!("running {cmd:?}"))?;
    Ok(out)
}

/// `extra` is appended after `run` (e.g. `--lockfile <path>`).
pub fn install(extra: &[String]) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    if cfg!(target_os = "macos") {
        install_macos(&exe, extra)
    } else if cfg!(target_os = "windows") {
        install_windows(&exe, extra)
    } else {
        bail!("install is only supported on macOS and Windows")
    }
}

pub fn uninstall() -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        uninstall_macos()
    } else if cfg!(target_os = "windows") {
        let out = run(Command::new("reg").args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "lol-remote",
            "/f",
        ]))?;
        if !out.status.success() {
            bail!("{}", String::from_utf8_lossy(&out.stderr));
        }
        println!("removed lol-remote from startup");
        Ok(())
    } else {
        bail!("uninstall is only supported on macOS and Windows")
    }
}

fn plist_path() -> anyhow::Result<PathBuf> {
    let home = directories::BaseDirs::new().context("no home directory")?;
    Ok(home
        .home_dir()
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn plist(exe: &Path, extra: &[String], logs: &Path) -> String {
    let args = std::iter::once(exe.display().to_string())
        .chain(std::iter::once("run".to_owned()))
        .chain(extra.iter().cloned())
        .map(|a| format!("        <string>{}</string>", xml_escape(&a)))
        .collect::<Vec<_>>()
        .join("\n");
    let out = xml_escape(&logs.join("lol-remote.log").display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
{args}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{out}</string>
    <key>StandardErrorPath</key>
    <string>{out}</string>
</dict>
</plist>
"#
    )
}

fn gui_domain() -> anyhow::Result<String> {
    let out = run(Command::new("id").arg("-u"))?;
    Ok(format!(
        "gui/{}",
        String::from_utf8_lossy(&out.stdout).trim()
    ))
}

fn install_macos(exe: &Path, extra: &[String]) -> anyhow::Result<()> {
    let logs = log_dir()?;
    std::fs::create_dir_all(&logs)?;
    let path = plist_path()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let domain = gui_domain()?;
    // Re-installing: unload the old agent first, ignoring "not loaded".
    let _ = run(Command::new("launchctl")
        .args(["bootout", &domain])
        .arg(&path));
    std::fs::write(&path, plist(exe, extra, &logs))?;
    let out = run(Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&path))?;
    if !out.status.success() {
        bail!(
            "launchctl bootstrap failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    println!("installed {}", path.display());
    println!("logs: {}", logs.join("lol-remote.log").display());
    println!("lol-remote now starts at login and waits for the LoL client.");
    Ok(())
}

fn uninstall_macos() -> anyhow::Result<()> {
    let path = plist_path()?;
    if !path.exists() {
        println!("not installed");
        return Ok(());
    }
    let _ = run(Command::new("launchctl")
        .args(["bootout", &gui_domain()?])
        .arg(&path));
    std::fs::remove_file(&path)?;
    println!("removed {}", path.display());
    Ok(())
}

fn install_windows(exe: &Path, extra: &[String]) -> anyhow::Result<()> {
    let mut value = format!("\"{}\" run", exe.display());
    for a in extra {
        value.push_str(&format!(" \"{a}\""));
    }
    let out = run(Command::new("reg").args([
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        "lol-remote",
        "/t",
        "REG_SZ",
        "/d",
        &value,
        "/f",
    ]))?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr));
    }
    println!("lol-remote will start at login: {value}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_contains_program_and_flags() {
        let p = plist(
            Path::new("/usr/local/bin/lol-remote"),
            &["--lockfile".into(), "/tmp/a&b".into()],
            Path::new("/tmp/logs"),
        );
        assert!(p.contains("<string>/usr/local/bin/lol-remote</string>"));
        assert!(p.contains("<string>run</string>"));
        assert!(p.contains("<string>/tmp/a&amp;b</string>"));
        assert!(p.contains("<key>KeepAlive</key>"));
    }
}
