//! `decklight soak`: Dauerlauf mit simulierten CDJs, alles in einem Prozess.
//!
//! Simulator (Demo-Szenario in Schleife) → Kern → Timecode (Art-Net), OSC-Trigger
//! und Webserver mit einem WebSocket-Client, der den Zustand wie die Oberfläche
//! mitliest. Alle `--sample-secs` werden CPU, RAM und Durchsatz als CSV-Zeile
//! geschrieben; am Ende prüft eine Auswertung, ob der Lauf stabil war.

use anyhow::{Context, Result};
use decklight_engine::EngineEvent;
use decklight_library::dbserver::server::DbServer;
use decklight_server::settings::Settings;
use decklight_server::{App, Server, ServerConfig, ShowData};
use decklight_show::Action;
use decklight_simulator::{Scenario, SimLibrary, Simulator, Target};
use futures_util::StreamExt;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(clap::Args)]
pub struct SoakArgs {
    /// Laufzeit in Minuten (Abnahme: 240).
    #[arg(long, default_value_t = 240.0)]
    minutes: f64,
    /// Abstand der Messungen in Sekunden.
    #[arg(long, default_value_t = 10)]
    sample_secs: u64,
    /// Messreihe als CSV.
    #[arg(long, default_value = "soak.csv")]
    out: PathBuf,
}

#[derive(Default)]
struct Counters {
    ws_frames: AtomicU64,
    osc_msgs: AtomicU64,
    tc_packets: AtomicU64,
    fired: AtomicU64,
    errors: AtomicU64,
    /// Größte Verspätung eines Triggers in Mikrosekunden.
    max_late_us: AtomicU64,
}

struct Sample {
    t_s: f64,
    cpu_pct: f32,
    rss_mb: f64,
    ws_fps: f64,
    osc: u64,
    tc_pps: f64,
    fired: u64,
}

/// Zählt UDP-Pakete, die an `sock` ankommen (OSC-Pult, Art-Net-Empfänger).
fn count_udp(sock: UdpSocket, counter: Arc<Counters>, pick: fn(&Counters) -> &AtomicU64) {
    sock.set_read_timeout(Some(Duration::from_millis(200))).ok();
    std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        loop {
            if sock.recv(&mut buf).is_ok() {
                pick(&counter).fetch_add(1, Ordering::Relaxed);
            }
        }
    });
}

pub async fn soak(args: SoakArgs) -> Result<()> {
    let counters = Arc::new(Counters::default());

    // Gegenstellen: Pult (OSC) und Art-Net-Empfänger auf diesem Rechner.
    let desk = UdpSocket::bind("127.0.0.1:0")?;
    let artnet = UdpSocket::bind("127.0.0.1:0")?;
    let desk_addr = desk.local_addr()?;
    let artnet_port = artnet.local_addr()?.port();
    count_udp(desk, counters.clone(), |c| &c.osc_msgs);
    count_udp(artnet, counters.clone(), |c| &c.tc_packets);

    let scenario = Scenario::demo();
    let db = DbServer::start(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        Arc::new(SimLibrary::from_scenario(&scenario)),
    )
    .await?;

    let mut settings = Settings::default();
    settings.network.passive = true;
    settings.network.port_base = 0;
    settings.network.db_port = db.query_port;
    settings.timecode_enabled = true;
    settings.timecode.artnet = Some(Ipv4Addr::LOCALHOST);
    settings.timecode.artnet_port = artnet_port;
    settings.actions.osc = Some(desk_addr);
    // Ohne MIDI-Gerät: nur die OSC-Aktionen der Beispiel-Trigger.
    let mut triggers = crate::example_triggers();
    for cues in triggers.tracks.values_mut() {
        for c in cues {
            c.actions.retain(|a| !matches!(a, Action::Midi { .. }));
        }
    }
    for g in &mut triggers.globals {
        g.actions.retain(|a| !matches!(a, Action::Midi { .. }));
    }
    let app = App::start(
        settings,
        ShowData {
            triggers,
            ..Default::default()
        },
    )
    .await?;
    let server = Server::start(
        app.clone(),
        ServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            ui_dir: None,
        },
    )
    .await?;

    let sim = tokio::spawn(decklight_simulator::run(
        Simulator::new(scenario, Ipv4Addr::LOCALHOST),
        Target {
            ip: Ipv4Addr::LOCALHOST,
            ports: app.core().link().local_ports(),
        },
        Duration::from_millis(5),
    ));

    // WebSocket-Client wie die Oberfläche.
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", server.addr))
        .await
        .context("WebSocket")?;
    let c = counters.clone();
    let ws_task = tokio::spawn(async move {
        let (_tx, mut rx) = ws.split();
        while let Some(Ok(m)) = rx.next().await {
            if m.to_text()
                .is_ok_and(|t| t.starts_with("{\"type\":\"state\""))
            {
                c.ws_frames.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    // Trigger-Ereignisse: Anzahl, Fehler, Verspätung.
    let mut events = app.core().subscribe();
    let c = counters.clone();
    let ev_task = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(s) => {
                    if let EngineEvent::TriggerFired { late_ms, error, .. } = s.event {
                        c.fired.fetch_add(1, Ordering::Relaxed);
                        if error.is_some() {
                            c.errors.fetch_add(1, Ordering::Relaxed);
                        }
                        let us = (late_ms.abs() * 1000.0) as u64;
                        c.max_late_us.fetch_max(us, Ordering::Relaxed);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => break,
            }
        }
    });

    let mut out = std::fs::File::create(&args.out)
        .with_context(|| format!("{} anlegen", args.out.display()))?;
    writeln!(out, "t_s;cpu_pct;rss_mb;ws_fps;osc_msgs;tc_pps;triggers")?;
    eprintln!(
        "Dauerlauf {:.0} min, Messung alle {} s → {}",
        args.minutes,
        args.sample_secs,
        args.out.display()
    );

    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    let refresh = |sys: &mut System| {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
    };
    refresh(&mut sys);
    let samples: Arc<Mutex<Vec<Sample>>> = Arc::default();
    let start = Instant::now();
    let end = start + Duration::from_secs_f64(args.minutes * 60.0);
    let mut tick = tokio::time::interval(Duration::from_secs(args.sample_secs.max(1)));
    tick.tick().await;
    let (mut last_ws, mut last_osc, mut last_tc, mut last_fired) = (0, 0, 0, 0);
    let mut last_t = Instant::now();
    while Instant::now() < end {
        tick.tick().await;
        refresh(&mut sys);
        let now = Instant::now();
        let dt = now.duration_since(last_t).as_secs_f64();
        last_t = now;
        let (cpu, rss) = sys
            .process(pid)
            .map(|p| (p.cpu_usage(), p.memory() as f64 / 1_048_576.0))
            .unwrap_or_default();
        let ws = counters.ws_frames.load(Ordering::Relaxed);
        let osc = counters.osc_msgs.load(Ordering::Relaxed);
        let tc = counters.tc_packets.load(Ordering::Relaxed);
        let fired = counters.fired.load(Ordering::Relaxed);
        let s = Sample {
            t_s: now.duration_since(start).as_secs_f64(),
            cpu_pct: cpu,
            rss_mb: rss,
            ws_fps: (ws - last_ws) as f64 / dt,
            osc: osc - last_osc,
            tc_pps: (tc - last_tc) as f64 / dt,
            fired: fired - last_fired,
        };
        (last_ws, last_osc, last_tc, last_fired) = (ws, osc, tc, fired);
        writeln!(
            out,
            "{:.0};{:.1};{:.1};{:.1};{};{:.1};{}",
            s.t_s, s.cpu_pct, s.rss_mb, s.ws_fps, s.osc, s.tc_pps, s.fired
        )?;
        out.flush()?;
        eprintln!(
            "{:>6.0} s  CPU {:>5.1} %  RAM {:>6.1} MB  WS {:>4.1}/s  TC {:>4.1}/s  Trigger {}",
            s.t_s, s.cpu_pct, s.rss_mb, s.ws_fps, s.tc_pps, fired
        );
        samples.lock().map_err(|_| anyhow::anyhow!("lock"))?.push(s);
    }
    sim.abort();
    ws_task.abort();
    ev_task.abort();

    let samples = samples.lock().map_err(|_| anyhow::anyhow!("lock"))?;
    let report = evaluate(&samples, &counters);
    println!("{report}");
    if report.ok {
        Ok(())
    } else {
        anyhow::bail!("Dauerlauf nicht bestanden")
    }
}

struct Report {
    ok: bool,
    lines: Vec<String>,
}

impl Report {
    fn check(&mut self, pass: bool, text: String) {
        self.lines
            .push(format!("{} {text}", if pass { "✓" } else { "✗" }));
        self.ok &= pass;
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for l in &self.lines {
            writeln!(f, "{l}")?;
        }
        write!(
            f,
            "{}",
            if self.ok {
                "BESTANDEN"
            } else {
                "NICHT BESTANDEN"
            }
        )
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Abnahme: RAM wächst nach dem Einschwingen um weniger als 25 MB, die Oberfläche
/// bekommt in jedem Messfenster mindestens 30 Zustände/s, der Timecode läuft,
/// Trigger feuern in jedem Demo-Durchlauf (3 min) und ohne Fehler, keiner davon
/// später als ein Frame (40 ms).
fn evaluate(samples: &[Sample], c: &Counters) -> Report {
    let mut r = Report {
        ok: true,
        lines: Vec::new(),
    };
    let n = samples.len();
    r.check(n >= 3, format!("{n} Messungen"));
    if n < 3 {
        return r;
    }
    // Erstes Fünftel (mind. 5 min) gilt als Einschwingen.
    let warm = samples
        .iter()
        .position(|s| s.t_s >= 300.0)
        .unwrap_or(n / 5)
        .min(n - 2);
    let tail = (n - warm) / 10 + 1;
    let early = median(
        samples[warm..warm + tail]
            .iter()
            .map(|s| s.rss_mb)
            .collect(),
    );
    let late = median(samples[n - tail..].iter().map(|s| s.rss_mb).collect());
    let max_rss = samples.iter().map(|s| s.rss_mb).fold(0.0, f64::max);
    r.check(
        late - early < 25.0,
        format!("RAM {early:.1} → {late:.1} MB (max {max_rss:.1} MB)"),
    );
    let cpu: Vec<f64> = samples.iter().map(|s| f64::from(s.cpu_pct)).collect();
    let avg_cpu = cpu.iter().sum::<f64>() / n as f64;
    let max_cpu = cpu.iter().copied().fold(0.0, f64::max);
    r.lines.push(format!(
        "  CPU Mittel {avg_cpu:.1} %, Spitze {max_cpu:.1} % (100 % = ein Kern)"
    ));
    let min_ws = samples[1..]
        .iter()
        .map(|s| s.ws_fps)
        .fold(f64::MAX, f64::min);
    r.check(
        min_ws >= 30.0,
        format!("Oberfläche mindestens {min_ws:.1} Zustände/s"),
    );
    let tc = samples.iter().filter(|s| s.tc_pps > 0.0).count();
    r.check(
        tc * 2 >= n,
        format!("Timecode lief in {tc} von {n} Messfenstern"),
    );
    // Jedes 4-Minuten-Fenster muss Trigger enthalten (Demo läuft alle 3 min).
    let window = 240.0;
    let mut silent = 0;
    let mut t0 = samples[0].t_s;
    while t0 + window <= samples[n - 1].t_s {
        let fired: u64 = samples
            .iter()
            .filter(|s| s.t_s > t0 && s.t_s <= t0 + window)
            .map(|s| s.fired)
            .sum();
        if fired == 0 {
            silent += 1;
        }
        t0 += window;
    }
    let fired = c.fired.load(Ordering::Relaxed);
    let osc: u64 = samples.iter().map(|s| s.osc).sum();
    r.check(
        silent == 0 && fired > 0,
        format!("{fired} Trigger, {osc} OSC-Nachrichten, {silent} stille Fenster"),
    );
    let errors = c.errors.load(Ordering::Relaxed);
    r.check(errors == 0, format!("{errors} Trigger-Fehler"));
    let late = c.max_late_us.load(Ordering::Relaxed) as f64 / 1000.0;
    r.check(
        late < 40.0,
        format!("größte Trigger-Verspätung {late:.1} ms"),
    );
    r
}
