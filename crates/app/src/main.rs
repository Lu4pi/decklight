//! Kommandozeile von DeckLight.

mod run;
mod serve;
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
    Watch(watch::WatchArgs),
    /// Alle PRO-DJ-LINK-Pakete in eine Datei mitschneiden (JSON Lines).
    Record(watch::RecordArgs),
    /// Mitschnitt durch die Engine spielen und die Ereignisse ausgeben.
    Replay {
        file: std::path::PathBuf,
        #[arg(long, default_value = "on-air")]
        master: String,
    },
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
        /// Abfrage-Port des simulierten dbservers (Metadaten, Cover, Wellen).
        #[arg(long, default_value_t = 12523)]
        db_port: u16,
    },
    /// Oberfläche und Webserver starten (Desktop-Browser, /monitor, /overlay).
    Serve(serve::ServeArgs),
    /// Timecode (Art-Net, MTC) live vom Master-Deck ausgeben.
    Run(run::RunArgs),
    /// Einen OSC-Befehl senden, z. B. zum Testen der grandMA3-Verbindung.
    Osc {
        /// Ziel, z. B. 192.168.1.50:8000
        #[arg(long)]
        to: std::net::SocketAddr,
        /// Präfix aus den OSC-Einstellungen des Pults, z. B. /gma3
        #[arg(long, default_value = "")]
        prefix: String,
        /// Befehlstext, z. B. "Go+ Exec 201"
        command: String,
    },
    /// MIDI-Ausgänge auflisten.
    MidiPorts,
    /// Demo-Ablaufplan als JSON ausgeben (Vorlage für eigene Szenarien).
    Scenario,
    /// Beispiel-Datei für Cues und Trigger ausgeben (für `run --triggers`).
    Triggers,
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
        Cmd::Watch(args) => watch::watch(args).await?,
        Cmd::Record(args) => watch::record(args).await?,
        Cmd::Replay { file, master } => watch::replay_file(file, &master)?,
        Cmd::Simulate {
            target,
            port_base,
            scenario,
            allow_network,
            db_port,
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
            let library =
                std::sync::Arc::new(decklight_simulator::SimLibrary::from_scenario(&scenario));
            let _db = match decklight_library::dbserver::server::DbServer::start(
                target.into(),
                db_port,
                library,
            )
            .await
            {
                Ok(db) => Some(db),
                Err(e) => {
                    eprintln!("Hinweis: dbserver auf Port {db_port} nicht gestartet ({e}); keine Metadaten.");
                    None
                }
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
        Cmd::Run(args) => run::run(args).await?,
        Cmd::Serve(args) => serve::serve(args).await?,
        Cmd::Osc {
            to,
            prefix,
            command,
        } => {
            decklight_outputs::osc::OscSender::new(to, &prefix)?.command(&command)?;
            eprintln!("Gesendet an {to}: \"{command}\"");
        }
        Cmd::MidiPorts => {
            for p in decklight_outputs::midi::output_ports()? {
                println!("{p}");
            }
        }
        Cmd::Scenario => println!("{}", serde_json::to_string_pretty(&Scenario::demo())?),
        Cmd::Triggers => println!("{}", serde_json::to_string_pretty(&example_triggers())?),
    }
    Ok(())
}

/// Beispiel: zwei Cues für den ersten Demo-Track und zwei globale Trigger.
fn example_triggers() -> decklight_show::TriggerTable {
    use decklight_outputs::MidiMessage;
    use decklight_show::{
        Action, Cue, CuePosition, DeckFilter, GlobalEvent, GlobalTrigger, TrackKey, TriggerTable,
    };
    let mut t = TriggerTable::default();
    let key = TrackKey("Sim Artist – Midnight Signal [6:00]".into());
    t.add_cue(
        &key,
        Cue::at_ms(
            "Intro-Licht",
            15_000.0,
            vec![Action::Osc {
                command: "Go+ Exec 201".into(),
            }],
        ),
    );
    t.add_cue(
        &key,
        Cue {
            position: CuePosition::Bar { bar: 33, beat: 1 },
            lead_ms: 40.0,
            color: Some([255, 40, 40]),
            ..Cue::at_ms(
                "Drop",
                0.0,
                vec![
                    Action::Osc {
                        command: "Go+ Exec 202".into(),
                    },
                    Action::Midi {
                        message: MidiMessage::NoteOn {
                            channel: 1,
                            note: 60,
                            velocity: 127,
                        },
                    },
                ],
            )
        },
    );
    t.add_global(GlobalTrigger {
        id: 0,
        name: "Deck wird Master".into(),
        on: GlobalEvent::MasterChanged,
        decks: DeckFilter::Any,
        actions: vec![Action::Osc {
            command: "Go+ Exec 10{deck}".into(),
        }],
        armed: true,
    });
    t.add_global(GlobalTrigger {
        id: 0,
        name: "Track geladen".into(),
        on: GlobalEvent::TrackLoaded,
        decks: DeckFilter::Any,
        actions: vec![Action::Osc {
            command: "Go+ Exec 11{deck}".into(),
        }],
        armed: true,
    });
    t
}
