//! Kommandozeile von DeckLight.

mod watch;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use decklight_prolink::{interfaces, PortConfig};
use decklight_simulator::{Scenario, Simulator, Target};
use std::net::Ipv4Addr;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "decklight",
    version,
    about = "Show-Steuerung für CDJs über PRO DJ LINK"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Netzwerkschnittstellen auflisten.
    Interfaces,
    /// Geräte und Decks live anzeigen.
    Watch(watch::Args),
    /// Virtuelle CDJs und DJM starten (Standard: nur an diesen Rechner).
    Simulate {
        /// Ziel-IP. 127.0.0.1 hält die Pakete auf diesem Rechner.
        #[arg(long, default_value = "127.0.0.1")]
        target: Ipv4Addr,
        /// Erster von drei Ports (Standard 50000 = echte PRO-DJ-LINK-Ports).
        #[arg(long, default_value_t = 50000)]
        port_base: u16,
        /// Ablaufplan als JSON (siehe `decklight scenario`); ohne Angabe die Demo.
        #[arg(long)]
        scenario: Option<std::path::PathBuf>,
        /// Auch an ein echtes Netz senden (nur ohne echte Player benutzen!).
        #[arg(long)]
        allow_network: bool,
    },
    /// Demo-Ablaufplan als JSON ausgeben (Vorlage für eigene Szenarien).
    Scenario,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,decklight=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    match Cli::parse().command {
        Cmd::Interfaces => {
            for i in interfaces()? {
                let mac = i.mac.map(|m| {
                    m.iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(":")
                });
                println!(
                    "{:<12} {:<15} Maske {:<15} Broadcast {:<15} {}{}",
                    i.name,
                    i.ip,
                    i.netmask,
                    i.broadcast,
                    mac.unwrap_or_default(),
                    if i.loopback { " (loopback)" } else { "" }
                );
            }
        }
        Cmd::Watch(args) => watch::run(args).await?,
        Cmd::Simulate {
            target,
            port_base,
            scenario,
            allow_network,
        } => {
            if !target.is_loopback() && !allow_network {
                bail!(
                    "Ziel {target} ist nicht dieser Rechner. Virtuelle Player 1–4 würden echte \
                     CDJs stören. Mit --allow-network bewusst freigeben."
                );
            }
            let scenario = match scenario {
                Some(path) => {
                    let text = std::fs::read_to_string(&path)
                        .with_context(|| format!("{} lesen", path.display()))?;
                    Scenario::from_json(&text).context("Szenario ist kein gültiges JSON")?
                }
                None => Scenario::demo(),
            };
            let sim = Simulator::new(scenario, target);
            let target = Target {
                ip: target,
                ports: PortConfig::from_base(port_base),
            };
            eprintln!(
                "Simulator sendet an {}:{}–{}. Beenden mit Strg+C.",
                target.ip, target.ports.announce, target.ports.status
            );
            tokio::select! {
                r = decklight_simulator::run(sim, target, Duration::from_millis(5)) => r?,
                _ = tokio::signal::ctrl_c() => {}
            }
        }
        Cmd::Scenario => println!("{}", serde_json::to_string_pretty(&Scenario::demo())?),
    }
    Ok(())
}
