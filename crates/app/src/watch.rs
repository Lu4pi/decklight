//! `decklight watch`, `record` und `replay`: Decks live im Terminal, Mitschnitte.

use anyhow::{Context, Result};
use decklight_engine::record::{read_records, replay, write_record, InputRecord};
use decklight_engine::{Engine, EngineSnapshot, MasterMode};
use decklight_prolink::{
    pick_interface, Link, LinkConfig, LinkEvent, PlayState, PortConfig, VirtualPlayer,
};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(clap::Args, Clone)]
pub struct NetArgs {
    /// Netzwerkschnittstelle (Name oder IP). Ohne Angabe die erste private.
    #[arg(long)]
    iface: Option<String>,
    /// Eigene Player-Nummer als virtueller Player (nie 1–4).
    #[arg(long, default_value_t = VirtualPlayer::DEFAULT_NUMBER,
          value_parser = clap::value_parser!(u8).range(5..=15))]
    player: u8,
    /// Nur zuhören, keine Keep-Alives senden. Echte CDJs schicken dann keinen Status.
    #[arg(long)]
    passive: bool,
    /// Erster von drei Ports (Standard 50000).
    #[arg(long, default_value_t = 50000)]
    port_base: u16,
}

#[derive(clap::Args)]
pub struct WatchArgs {
    #[command(flatten)]
    net: NetArgs,
    /// Master-Auswahl: on-air, tempo-master oder eine Decknummer.
    #[arg(long, default_value = "on-air")]
    master: String,
    /// Jedes Ereignis als JSON-Zeile ausgeben statt der Tabelle.
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct RecordArgs {
    #[command(flatten)]
    net: NetArgs,
    /// Zieldatei (JSON Lines). Später mit `decklight replay` abspielbar.
    out: PathBuf,
}

pub fn parse_master(s: &str) -> Result<MasterMode> {
    Ok(match s {
        "on-air" | "onair" => MasterMode::OnAir,
        "tempo-master" | "tempo" => MasterMode::TempoMaster,
        n => MasterMode::Manual(
            n.parse()
                .context("Master: on-air, tempo-master oder Decknummer")?,
        ),
    })
}

pub fn link_config(net: &NetArgs) -> Result<LinkConfig> {
    let virtual_player = if net.passive {
        None
    } else {
        let iface = pick_interface(net.iface.as_deref()).context(
            "Keine Schnittstelle zum CDJ-Netz gefunden. --iface angeben oder mit --passive nur zuhören",
        )?;
        eprintln!(
            "Melde mich als Player {} auf {} ({}) an.",
            net.player, iface.name, iface.ip
        );
        Some(VirtualPlayer::on_interface(&iface, net.player))
    };
    Ok(LinkConfig {
        ports: PortConfig::from_base(net.port_base),
        virtual_player,
        ..Default::default()
    })
}

async fn start_link(net: &NetArgs) -> Result<Link> {
    Link::start(link_config(net)?)
        .await
        .context("PRO-DJ-LINK-Ports konnten nicht geöffnet werden")
}

pub async fn watch(args: WatchArgs) -> Result<()> {
    let link = start_link(&args.net).await?;
    let mut events = link.subscribe();
    let mut engine = Engine::default();
    engine.set_master_mode(parse_master(&args.master)?);
    let start = Instant::now();
    let mut redraw = tokio::time::interval(Duration::from_millis(100));
    let tty = std::io::stdout().is_terminal();

    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Ok(ev) => {
                    let at = match &ev { LinkEvent::Packet { at, .. } => *at, _ => Instant::now() };
                    let t = at.saturating_duration_since(start).as_secs_f64();
                    for e in engine.handle_link_event(t, &ev) {
                        if args.json {
                            println!("{}", serde_json::to_string(&decklight_engine::Stamped { t, event: e })?);
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "Ereignisse übersprungen");
                }
                Err(_) => break,
            },
            _ = redraw.tick() => {
                let t = start.elapsed().as_secs_f64();
                for e in engine.tick(t) {
                    if args.json {
                        println!("{}", serde_json::to_string(&decklight_engine::Stamped { t, event: e })?);
                    }
                }
                if !args.json {
                    draw(&engine.snapshot(t), tty)?;
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

pub async fn record(args: RecordArgs) -> Result<()> {
    let link = start_link(&args.net).await?;
    let mut events = link.subscribe();
    let file = std::fs::File::create(&args.out)
        .with_context(|| format!("{} anlegen", args.out.display()))?;
    let mut w = std::io::BufWriter::new(file);
    let start = Instant::now();
    let mut count = 0u64;
    eprintln!(
        "Zeichne auf nach {}. Beenden mit Strg+C.",
        args.out.display()
    );
    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Ok(LinkEvent::Packet { at, packet, .. }) => {
                    let t = at.saturating_duration_since(start).as_secs_f64();
                    write_record(&mut w, &InputRecord { t, packet })?;
                    count += 1;
                    // Regelmäßig schreiben, damit ein hart beendeter Mitschnitt brauchbar bleibt.
                    if count % 20 == 0 {
                        w.flush()?;
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("Warnung: {n} Pakete verloren");
                }
                Err(_) => break,
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    w.flush()?;
    eprintln!("{count} Pakete gespeichert.");
    Ok(())
}

pub fn replay_file(path: PathBuf, master: &str) -> Result<()> {
    let file = std::fs::File::open(&path).with_context(|| format!("{} öffnen", path.display()))?;
    let records = read_records(std::io::BufReader::new(file))?;
    let mut engine = Engine::default();
    engine.set_master_mode(parse_master(master)?);
    let mut out = std::io::stdout().lock();
    for s in replay(&mut engine, &records) {
        writeln!(out, "{}", serde_json::to_string(&s)?)?;
    }
    Ok(())
}

fn draw(snap: &EngineSnapshot, tty: bool) -> Result<()> {
    let mut out = String::new();
    if tty {
        out.push_str("\x1b[2J\x1b[H");
    }
    out.push_str(&format!("Geräte: {}\n", snap.devices.len()));
    for d in &snap.devices {
        out.push_str(&format!(
            "  #{:<3} {:<14} {:<10} {}\n",
            d.number,
            d.name,
            format!("{:?}", d.kind),
            d.ip
        ));
    }
    out.push_str(&format!(
        "\nMaster: {} ({:?})\n\nDeck  Zustand      Track   BPM      Pitch    Position              Beat  Master OnAir\n",
        snap.master.map_or("-".into(), |m| m.to_string()),
        snap.master_mode
    ));
    for d in &snap.decks {
        let state = match d.play_state {
            PlayState::Unknown(b) => format!("?{b:#04x}"),
            other => format!("{other:?}"),
        };
        let pos = match (d.position_ms, d.track_length_ms) {
            (Some(p), Some(l)) => format!("{} / {}", clock(p), clock(f64::from(l))),
            (Some(p), None) => format!("{}{}", clock(p), if d.precise { "" } else { " ~" }),
            _ => "--".into(),
        };
        out.push_str(&format!(
            "{:<5} {:<12} {:<7} {:<8} {:<+8.2} {:<21} {:<5} {:<6} {}\n",
            d.number,
            state,
            d.track.map_or("-".into(), |t| t.rekordbox_id.to_string()),
            d.effective_bpm.map_or("--".into(), |b| format!("{b:.2}")),
            d.pitch_percent,
            pos,
            d.beat_in_bar,
            if snap.master == Some(d.number) {
                "◆"
            } else if d.tempo_master {
                "T"
            } else {
                ""
            },
            if d.on_air { "ON AIR" } else { "" },
        ));
    }
    if !tty {
        out.push_str("----\n");
    }
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(out.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn clock(ms: f64) -> String {
    let ms = ms.max(0.0) as u64;
    format!("{}:{:02}.{:03}", ms / 60_000, (ms / 1000) % 60, ms % 1000)
}
