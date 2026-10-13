//! Windows-Firewall: CDJ-Netze ohne Router gelten unter Windows als „Nicht
//! identifiziertes Netzwerk“ und damit als öffentlich. Hat man DeckLight beim
//! ersten Start nur für private Netze erlaubt, verstummen die Player nach
//! 20–30 s. Eine eigene Regel für alle Profile verhindert das.

/// Name der Regel; der Installer legt dieselbe an.
pub const RULE: &str = "DeckLight";

/// `Some(true)`, wenn die Regel existiert; außerhalb von Windows `None`.
pub fn check() -> Option<bool> {
    imp::check()
}

/// Regel anlegen (fragt per UAC nach Administratorrechten). Entfernt vorher
/// alle Regeln für dieses Programm, auch die Sperre, die Windows nach
/// „Abbrechen“ im Firewall-Dialog anlegt; eine Sperre ginge sonst vor.
pub fn install() -> anyhow::Result<()> {
    imp::install()
}

/// Argumente für `cmd /c`: alte Regeln des Programms löschen, neue anlegen.
pub fn netsh_script(exe: &str) -> String {
    format!(
        "netsh advfirewall firewall delete rule name=all program=\"{exe}\" & \
         netsh advfirewall firewall add rule name={RULE} dir=in action=allow \
         program=\"{exe}\" enable=yes profile=any"
    )
}

#[cfg(windows)]
mod imp {
    use super::{netsh_script, RULE};
    use anyhow::{ensure, Context};
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const NO_WINDOW: u32 = 0x0800_0000;

    pub fn check() -> Option<bool> {
        let out = Command::new("netsh")
            .args([
                "advfirewall",
                "firewall",
                "show",
                "rule",
                &format!("name={RULE}"),
            ])
            .creation_flags(NO_WINDOW)
            .output()
            .ok()?;
        Some(out.status.success())
    }

    pub fn install() -> anyhow::Result<()> {
        let exe = std::env::current_exe().context("Programmpfad unbekannt")?;
        let script = netsh_script(&exe.display().to_string());
        let ps = format!(
            "Start-Process -FilePath cmd.exe -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '/c {}'",
            script.replace('\'', "''")
        );
        let status = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
            .creation_flags(NO_WINDOW)
            .status()
            .context("PowerShell nicht gefunden")?;
        ensure!(status.success(), "Freigabe abgebrochen");
        ensure!(check() == Some(true), "Firewall-Regel wurde nicht angelegt");
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn check() -> Option<bool> {
        None
    }

    pub fn install() -> anyhow::Result<()> {
        anyhow::bail!("Nur unter Windows nötig")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn script_removes_old_rules_before_adding() {
        let s = super::netsh_script(r"C:\Program Files\DeckLight\decklight-desktop.exe");
        let del = s.find("delete rule name=all").unwrap();
        let add = s.find("add rule name=DeckLight").unwrap();
        assert!(del < add);
        assert!(s.contains("profile=any"));
        assert!(s.contains(r#"program="C:\Program Files\DeckLight\decklight-desktop.exe""#));
    }
}
