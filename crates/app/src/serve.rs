//! `decklight serve`: Kern plus Webserver mit Oberfläche, `/monitor` und `/overlay`.

use anyhow::Result;
use decklight_server::files::{load, Persist};
use decklight_server::settings::Settings;
use decklight_server::{App, Server, ServerConfig, ShowData, DEFAULT_PORT};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Einstellungen (JSON). Wird angelegt und bei Änderungen gespeichert.
    #[arg(long, default_value = "decklight-settings.json")]
    settings: PathBuf,
    /// Show-Daten (Offsets, Cues, Trigger) als JSON. Wird alle 5 s gesichert.
    #[arg(long, default_value = "decklight-show.json")]
    show: PathBuf,
    /// Port des Webservers.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Nur auf diesem Rechner erreichbar (sonst im ganzen Netz für /monitor).
    #[arg(long)]
    local_only: bool,
    /// Gebaute Oberfläche (Standard: ui/dist).
    #[arg(long, default_value = "ui/dist")]
    ui_dir: PathBuf,
    /// Nur zuhören (überschreibt die Einstellungen; zum Testen mit dem Simulator).
    #[arg(long)]
    passive: bool,
    /// Erster der drei PRO-DJ-LINK-Ports (überschreibt die Einstellungen).
    #[arg(long)]
    port_base: Option<u16>,
    /// dbserver-Abfrage-Port (überschreibt die Einstellungen).
    #[arg(long)]
    db_port: Option<u16>,
}

pub async fn serve(args: ServeArgs) -> Result<()> {
    let mut settings: Settings = load(&args.settings)?;
    if args.passive {
        settings.network.passive = true;
    }
    if let Some(p) = args.port_base {
        settings.network.port_base = p;
    }
    if let Some(p) = args.db_port {
        settings.network.db_port = p;
    }
    let show: ShowData = load(&args.show)?;
    let app = App::start(settings, show).await?;
    let ip = if args.local_only {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };
    let server = Server::start(
        app.clone(),
        ServerConfig {
            bind: SocketAddr::new(ip, args.port),
            ui_dir: Some(args.ui_dir.clone()),
        },
    )
    .await?;
    eprintln!(
        "DeckLight läuft: http://127.0.0.1:{0}/  Monitor: /monitor  Overlay: /overlay",
        server.addr.port()
    );
    let persist = Arc::new(Persist::new(app, args.settings.clone(), args.show.clone()));
    let task = tokio::spawn(persist.clone().run(Duration::from_secs(5)));
    tokio::signal::ctrl_c().await?;
    task.abort();
    persist.save(true)?;
    eprintln!(
        "Gespeichert: {}, {}",
        args.show.display(),
        args.settings.display()
    );
    Ok(())
}
