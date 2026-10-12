//! Einstellungen der App: Netz, Master-Auswahl, Ausgänge, Webserver.

use anyhow::{Context, Result};
use decklight_core::{ActionOutConfig, CoreConfig, TimecodeOutConfig};
use decklight_engine::MasterMode;
use decklight_prolink::{pick_interface, LinkConfig, PortConfig, VirtualPlayer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    /// Schnittstelle (Name oder IP). `None`: die erste private.
    pub iface: Option<String>,
    /// Eigene Player-Nummer (5–15, nie 1–4).
    pub player: u8,
    /// Nur zuhören. Echte CDJs schicken dann keinen Status.
    pub passive: bool,
    pub port_base: u16,
    pub db_port: u16,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            iface: None,
            player: VirtualPlayer::DEFAULT_NUMBER,
            passive: false,
            port_base: 50000,
            db_port: 12523,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSettings {
    /// PIN für Zugriffe aus dem Netz (`/monitor`, `/overlay`). Leer: offen.
    pub pin: Option<String>,
    /// Befehle auch aus dem Netz annehmen (sonst nur vom eigenen Rechner).
    pub allow_lan_control: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub network: NetworkSettings,
    pub master_mode: MasterMode,
    pub timecode_enabled: bool,
    pub timecode: TimecodeOutConfig,
    pub actions: ActionOutConfig,
    pub web: WebSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            network: NetworkSettings::default(),
            master_mode: MasterMode::OnAir,
            timecode_enabled: true,
            timecode: TimecodeOutConfig::default(),
            actions: ActionOutConfig::default(),
            web: WebSettings::default(),
        }
    }
}

impl Settings {
    pub fn link_config(&self) -> Result<LinkConfig> {
        let n = &self.network;
        anyhow::ensure!(
            (5..=15).contains(&n.player),
            "Player-Nummer {} nicht erlaubt (5–15)",
            n.player
        );
        let virtual_player = if n.passive {
            None
        } else {
            let iface = pick_interface(n.iface.as_deref())
                .context("Keine Schnittstelle zum CDJ-Netz gefunden")?;
            Some(VirtualPlayer::on_interface(&iface, n.player))
        };
        Ok(LinkConfig {
            ports: PortConfig::from_base(n.port_base),
            virtual_player,
            ..Default::default()
        })
    }

    pub fn timecode_config(&self) -> Option<TimecodeOutConfig> {
        self.timecode_enabled.then(|| self.timecode.clone())
    }

    /// Kern-Konfiguration ohne Show-Daten.
    pub fn core_config(&self) -> Result<CoreConfig> {
        Ok(CoreConfig {
            link: self.link_config()?,
            master_mode: self.master_mode,
            timecode: self.timecode_config(),
            db_query_port: Some(self.network.db_port),
            actions: self.actions.clone(),
            ..Default::default()
        })
    }

    /// Geänderte Netz-Einstellungen brauchen einen Neustart des Kerns.
    pub fn needs_restart(&self, new: &Settings) -> bool {
        self.network != new.network
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_fills_defaults() {
        let s: Settings =
            serde_json::from_str(r#"{"network":{"passive":true},"web":{"pin":"1234"}}"#).unwrap();
        assert!(s.network.passive);
        assert_eq!(s.network.player, 7);
        assert_eq!(s.web.pin.as_deref(), Some("1234"));
        assert!(s.timecode_enabled);
        assert!(s.link_config().unwrap().virtual_player.is_none());
    }

    #[test]
    fn players_one_to_four_are_refused() {
        let mut s = Settings::default();
        s.network.passive = true;
        s.network.player = 3;
        assert!(s.link_config().is_err());
    }
}
