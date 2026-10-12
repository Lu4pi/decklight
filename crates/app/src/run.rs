//! `decklight run`: Timecode-Ausgang live, gesteuert vom Master-Deck.

use crate::watch::{link_config, parse_master, NetArgs};
use anyhow::{bail, Context, Result};
use decklight_core::{Core, CoreConfig, TimecodeOutConfig};
use decklight_outputs::{FrameRate, GeneratorConfig, StopMode};
use decklight_show::OffsetTable;
use std::io::{IsTerminal, Write};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(clap::Args)]
pub struct RunArgs {
    #[command(flatten)]
    net: NetArgs,
    /// Art-Net-Timecode an diese Adresse (Broadcast wie 2.255.255.255 oder Pult-IP).
    #[arg(long)]
    artnet: Option<Ipv4Addr>,
    /// MTC an diesen MIDI-Ausgang (Teil des Namens, siehe `decklight midi-ports`).
    #[arg(long)]
    mtc: Option<String>,
    /// Bildrate: 24, 25, 29.97 (Drop-Frame) oder 30.
    #[arg(long, default_value = "25")]
    fps: String,
    /// Im Stillstand letzten Wert weiter senden (freeze) oder verstummen (stop).
    #[arg(long, default_value = "freeze")]
    stop_mode: String,
    /// Master-Auswahl: on-air, tempo-master oder eine Decknummer.
    #[arg(long, default_value = "on-air")]
    master: String,
    /// Offsets aus CSV laden (key;label;offset); beim Beenden wird sie aktualisiert.
    #[arg(long)]
    offsets: Option<PathBuf>,
}

pub async fn run(args: RunArgs) -> Result<()> {
    let rate = FrameRate::parse(&args.fps).context("--fps: 24, 25, 29.97 oder 30")?;
    let stop_mode = match args.stop_mode.as_str() {
        "freeze" => StopMode::Freeze,
        "stop" => StopMode::Stop,
        other => bail!("--stop-mode {other}: freeze oder stop"),
    };
    let mut offsets = OffsetTable::default();
    if let Some(path) = &args.offsets {
        if path.exists() {
            let text = std::fs::read_to_string(path)?;
            let n = offsets.import_csv(&text, rate)?;
            eprintln!("{n} Offsets aus {} geladen.", path.display());
        }
    }
    if args.artnet.is_none() && args.mtc.is_none() {
        eprintln!("Hinweis: weder --artnet noch --mtc angegeben, Timecode wird nur angezeigt.");
    }
    let core = Core::start(CoreConfig {
        link: link_config(&args.net)?,
        master_mode: parse_master(&args.master)?,
        timecode: Some(TimecodeOutConfig {
            generator: GeneratorConfig { rate, stop_mode },
            artnet: args.artnet,
            mtc_port: args.mtc.clone(),
            ..Default::default()
        }),
        offsets,
        ..Default::default()
    })
    .await
    .context("Start fehlgeschlagen")?;

    let tty = std::io::stdout().is_terminal();
    let mut tick = tokio::time::interval(Duration::from_millis(40));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let line = core.with(|s| {
                    let tc = &s.timecode;
                    format!(
                        "TC {}  Deck {}  Track {}  {}  Fehler {}",
                        tc.timecode.map_or("--:--:--:--".into(), |t| t.to_string()),
                        tc.deck.map_or("-".into(), |d| d.to_string()),
                        tc.track.as_ref().map_or("-".into(), |k| k.to_string()),
                        if tc.running { "läuft" } else { "steht" },
                        tc.errors,
                    )
                }).unwrap_or_default();
                let mut out = std::io::stdout().lock();
                if tty { write!(out, "\r\x1b[2K{line}")?; } else { writeln!(out, "{line}")?; }
                out.flush()?;
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    println!();
    if let Some(path) = &args.offsets {
        let csv = core.with(|s| s.offsets.to_csv(rate)).unwrap_or_default();
        std::fs::write(path, csv).with_context(|| format!("{} schreiben", path.display()))?;
        eprintln!("Offsets gespeichert in {}.", path.display());
    }
    Ok(())
}
