//! Kommandozeile von DeckLight.

mod run;
mod serve;
mod soak;
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
        /// Audiodateien (WAV, AIFF, FLAC, MP3, M4A) als DJ-Set auf Deck 1 und 2.
        /// Tempo, erster Beat und Wellenform kommen aus der Analyse der Dateien.
        #[arg(long, num_args = 1.., conflicts_with_all = ["scenario", "replay"])]
        audio: Vec<std::path::PathBuf>,
        /// Rekordbox-Export (Stick oder Kopie, Ordner mit `PIONEER`) als DJ-Set:
        /// echte Titel, Beatgrids, Wellenformen, Cues und Cover.
        #[arg(long, conflicts_with_all = ["scenario", "replay", "audio"])]
        usb: Option<std::path::PathBuf>,
        /// Höchstens so viele Tracks aus dem Export (in Reihenfolge der IDs).
        #[arg(long, default_value_t = 6, requires = "usb")]
        tracks: usize,
        /// Mitschnitt abspielen: Wireshark (.pcap, .pcapng) oder `decklight record` (.jsonl).
        #[arg(long, conflicts_with = "scenario")]
        replay: Option<std::path::PathBuf>,
        /// Abspielgeschwindigkeit des Mitschnitts.
        #[arg(long, default_value_t = 1.0, requires = "replay")]
        speed: f64,
        /// Mitschnitt nur einmal abspielen (Standard: Schleife).
        #[arg(long, requires = "replay")]
        once: bool,
    },
    /// Oberfläche und Webserver starten (Desktop-Browser, /monitor, /overlay).
    Serve(serve::ServeArgs),
    /// Dauerlauf mit simulierten CDJs: CPU, RAM und Durchsatz messen.
    Soak(soak::SoakArgs),
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
            audio,
            usb,
            tracks,
            replay,
            speed,
            once,
        } => {
            if !target.is_loopback() && !allow_network {
                bail!(
                    "Ziel {target} ist nicht dieser Rechner. Virtuelle Player 1–4 würden echte \
                     CDJs stören. Mit --allow-network bewusst freigeben."
                );
            }
            if let Some(path) = replay {
                return replay_capture(&path, target, port_base, speed, !once).await;
            }
            let mut scenario = match scenario {
                Some(path) => {
                    let text = std::fs::read_to_string(&path)
                        .with_context(|| format!("{} lesen", path.display()))?;
                    Scenario::from_json(&text).context("Szenario ist kein gültiges JSON")?
                }
                None if usb.is_some() => Scenario::set_list(Vec::new()),
                None if !audio.is_empty() => Scenario::set_list(
                    audio
                        .iter()
                        .enumerate()
                        .map(|(i, f)| decklight_simulator::SimTrack {
                            id: 2001 + i as u32,
                            title: String::new(),
                            artist: String::new(),
                            length_ms: 0,
                            bpm: 120.0,
                            first_beat_ms: 0.0,
                            file: Some(f.clone()),
                        })
                        .collect(),
                ),
                None => Scenario::demo(),
            };
            let has_audio = scenario.steps.iter().any(|s| {
                matches!(&s.command, decklight_simulator::Command::Load { track, .. } if track.file.is_some())
            });
            if has_audio {
                eprintln!("Analysiere Audiodateien …");
            }
            let mut library = decklight_simulator::SimLibrary::load_audio(&mut scenario)?;
            if let Some(root) = &usb {
                let ex = decklight_library::export::Export::open(root)?;
                let mut ids: Vec<u32> = ex.db.tracks.keys().copied().collect();
                ids.sort_unstable();
                let mut set = Vec::new();
                for id in ids.into_iter().take(tracks.max(1)) {
                    let Some(data) = ex.track(id) else { continue };
                    let t = decklight_simulator::SimTrack::from_track_data(&data);
                    eprintln!(
                        "  {}",
                        decklight_simulator::audio::describe(
                            &data.metadata,
                            t.bpm,
                            t.first_beat_ms
                        )
                    );
                    library.insert(id, data);
                    set.push(t);
                }
                anyhow::ensure!(!set.is_empty(), "Keine Tracks im Export {}", root.display());
                scenario = Scenario::set_list(set);
            }
            if has_audio {
                // Set-Liste neu planen, jetzt mit echten Längen.
                if !audio.is_empty() {
                    let tracks: Vec<_> = scenario
                        .steps
                        .iter()
                        .filter_map(|s| match &s.command {
                            decklight_simulator::Command::Load { track, .. } => Some(track.clone()),
                            _ => None,
                        })
                        .collect();
                    scenario = Scenario::set_list(tracks);
                }
                for s in &scenario.steps {
                    if let decklight_simulator::Command::Load { track, deck } = &s.command {
                        if let Some(d) = library.get(track.id) {
                            eprintln!(
                                "  Deck {deck}: {}",
                                decklight_simulator::audio::describe(
                                    &d.metadata,
                                    track.bpm,
                                    track.first_beat_ms
                                )
                            );
                        }
                    }
                }
            }
            let library = std::sync::Arc::new(library);
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
        Cmd::Soak(args) => soak::soak(args).await?,
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
/// Mitschnitt an die eigenen Ports schicken (pcap, pcapng oder JSON Lines).
async fn replay_capture(
    path: &std::path::Path,
    target: Ipv4Addr,
    port_base: u16,
    speed: f64,
    repeat: bool,
) -> Result<()> {
    use decklight_simulator::capture::{self, Captured};
    let data = std::fs::read(path).with_context(|| format!("{} lesen", path.display()))?;
    let packets: Vec<Captured> = if data.first() == Some(&b'{') {
        let records = decklight_engine::record::read_records(std::io::BufReader::new(&data[..]))?;
        records
            .iter()
            .filter_map(|r| {
                let (channel, bytes) = decklight_prolink::encode::packet(&r.packet)?;
                Some(Captured {
                    t: r.t,
                    channel,
                    bytes,
                })
            })
            .collect()
    } else {
        capture::parse_pcap(&data).with_context(|| format!("{} lesen", path.display()))?
    };
    let secs = packets.last().map_or(0.0, |p| p.t);
    eprintln!(
        "Spiele {} PRO-DJ-LINK-Pakete ({:.0} s) an {target}:{}–{}{}. Beenden mit Strg+C.",
        packets.len(),
        secs,
        port_base,
        port_base + 2,
        if repeat { " in Schleife" } else { "" }
    );
    let target = Target {
        ip: target,
        ports: PortConfig::from_base(port_base),
    };
    tokio::select! {
        r = capture::play(packets, target, speed, repeat) => r?,
        _ = tokio::signal::ctrl_c() => {}
    }
    Ok(())
}

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
