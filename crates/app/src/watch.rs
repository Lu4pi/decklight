//! `decklight watch`: Geräte und Decks live im Terminal.

use anyhow::{Context, Result};
use decklight_prolink::packet::PlayState;
use decklight_prolink::{
    pick_interface, Link, LinkConfig, LinkEvent, LinkSnapshot, PlayerSnapshot, PortConfig,
    VirtualPlayer,
};
use std::io::{IsTerminal, Write};
use std::time::Duration;

#[derive(clap::Args)]
pub struct Args {
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
    /// Jedes Ereignis als JSON-Zeile ausgeben statt der Tabelle.
    #[arg(long)]
    json: bool,
}

pub async fn run(args: Args) -> Result<()> {
    let virtual_player = if args.passive {
        None
    } else {
        let iface = pick_interface(args.iface.as_deref()).context(
            "Keine Schnittstelle zum CDJ-Netz gefunden. --iface angeben oder mit --passive nur zuhören",
        )?;
        eprintln!(
            "Melde mich als Player {} auf {} ({}) an.",
            args.player, iface.name, iface.ip
        );
        Some(VirtualPlayer::on_interface(&iface, args.player))
    };
    let link = Link::start(LinkConfig {
        ports: PortConfig::from_base(args.port_base),
        virtual_player,
        ..Default::default()
    })
    .await
    .context("PRO-DJ-LINK-Ports konnten nicht geöffnet werden")?;
    let mut events = link.subscribe();
    let mut snap = LinkSnapshot::default();
    let mut redraw = tokio::time::interval(Duration::from_millis(250));
    let tty = std::io::stdout().is_terminal();

    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Ok(ev) => {
                    if args.json {
                        println!("{}", serde_json::to_string(&ev)?);
                    }
                    if let LinkEvent::NumberConflict { number, other } = &ev {
                        eprintln!("Achtung: {} ({}) nutzt ebenfalls Nummer {number}", other.name, other.ip);
                    }
                    snap.apply(&ev);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "Ereignisse übersprungen");
                }
                Err(_) => break,
            },
            _ = redraw.tick(), if !args.json => draw(&snap, tty)?,
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

fn draw(snap: &LinkSnapshot, tty: bool) -> Result<()> {
    let mut out = String::new();
    if tty {
        out.push_str("\x1b[2J\x1b[H");
    }
    out.push_str(&format!("Geräte: {}\n", snap.devices.len()));
    for d in snap.devices.values() {
        out.push_str(&format!(
            "  #{:<3} {:<14} {:<10} {:<15} {}\n",
            d.number,
            d.name,
            format!("{:?}", d.kind),
            d.ip,
            d.mac_string()
        ));
    }
    out.push_str(
        "\nDeck  Zustand      Track   BPM      Pitch    Position           Beat  Master OnAir\n",
    );
    for p in snap.players.values() {
        out.push_str(&deck_line(p, snap));
        out.push('\n');
    }
    if let Some(m) = &snap.mixer {
        out.push_str(&format!(
            "\nMixer {}: {} BPM\n",
            m.name,
            m.bpm.map_or("--".into(), |b| format!("{b:.1}"))
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

fn deck_line(p: &PlayerSnapshot, snap: &LinkSnapshot) -> String {
    let Some(s) = &p.status else {
        return format!("{:<5} (noch kein Status)", p.number);
    };
    let position = p.position.as_ref().map_or_else(
        || "--".to_string(),
        |pp| {
            format!(
                "{} / {}",
                clock(pp.position_ms),
                clock(pp.track_length_s * 1000)
            )
        },
    );
    let state = match s.play_state {
        PlayState::Unknown(b) => format!("?{b:#04x}"),
        other => format!("{other:?}"),
    };
    format!(
        "{:<5} {:<12} {:<7} {:<8} {:<+8.2} {:<18} {:<5} {:<6} {}",
        p.number,
        state,
        if s.rekordbox_id == 0 {
            "-".to_string()
        } else {
            s.rekordbox_id.to_string()
        },
        s.effective_bpm().map_or("--".into(), |b| format!("{b:.2}")),
        s.pitch_percent,
        position,
        s.beat_number
            .map_or("-".into(), |b| format!("{b}.{}", s.beat_in_bar)),
        if s.master { "M" } else { "" },
        if p.on_air(snap.on_air.as_ref()) {
            "ON AIR"
        } else {
            ""
        },
    )
}

fn clock(ms: u32) -> String {
    format!("{}:{:02}.{:03}", ms / 60_000, (ms / 1000) % 60, ms % 1000)
}
