//! Webserver von DeckLight.
//!
//! Liefert die Oberfläche aus (Desktop-App, `/monitor`, `/overlay`) und streamt den
//! Zustand per WebSocket (`/ws`): 40 Zustands-Frames pro Sekunde, alle Ereignisse,
//! zweimal pro Sekunde Monitoring-Daten und die Show-Daten bei jeder Änderung.
//! Befehle (`POST /api/command`) nimmt er nur vom eigenen Rechner an, außer es ist
//! ausdrücklich erlaubt. Mit PIN brauchen Zugriffe aus dem Netz `?pin=…`.

pub mod command;
pub mod files;
pub mod library;
pub mod settings;
pub mod view;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use command::{Command, Reply};
use decklight_core::Core;
use decklight_show::{OffsetTable, TrackKey, TriggerTable};
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

pub use command::Command as UiCommand;
pub use settings::{NetworkSettings, WebSettings};

pub const DEFAULT_PORT: u16 = 8750;

/// Show-Daten, die einen Neustart des Kerns überleben.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShowData {
    #[serde(default)]
    pub offsets: OffsetTable,
    #[serde(default)]
    pub triggers: TriggerTable,
}

/// Laufender Kern plus Einstellungen. Wird von Server und Desktop-App geteilt.
pub struct App {
    core: RwLock<Arc<Core>>,
    settings: Mutex<Settings>,
    events: broadcast::Sender<Arc<str>>,
    forwarder: Mutex<Option<JoinHandle<()>>>,
    restart: tokio::sync::Mutex<()>,
    library: Mutex<Option<Arc<library::Library>>>,
    show_name: Mutex<String>,
}

impl App {
    pub async fn start(settings: Settings, show: ShowData) -> Result<Arc<Self>> {
        let core = match start_core(&settings, show.clone()).await {
            Ok(c) => c,
            Err(e) if !settings.network.passive => {
                // Ohne passende Schnittstelle trotzdem starten, damit die Oberfläche
                // erreichbar bleibt und man die Einstellung korrigieren kann.
                tracing::warn!(error = %format!("{e:#}"), "Start als virtueller Player fehlgeschlagen, nur zuhören");
                let mut fallback = settings.clone();
                fallback.network.passive = true;
                let core = start_core(&fallback, show).await?;
                core.with(|s| {
                    s.monitor.warn(
                        decklight_core::Level::Error,
                        format!("{e:#}. DeckLight hört nur zu, bis unter Ausgänge eine Schnittstelle gewählt ist."),
                    )
                });
                core
            }
            Err(e) => return Err(e),
        };
        let (events, _) = broadcast::channel(1024);
        let app = Arc::new(Self {
            core: RwLock::new(core),
            settings: Mutex::new(settings),
            events,
            forwarder: Mutex::new(None),
            restart: tokio::sync::Mutex::new(()),
            library: Mutex::new(None),
            show_name: Mutex::new("Neue Show".into()),
        });
        app.forward_events();
        Ok(app)
    }

    pub fn core(&self) -> Arc<Core> {
        self.core
            .read()
            .map(|c| c.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// Timecode an/aus und Bildrate, ohne die ganzen Einstellungen zu kopieren.
    fn timecode_view_settings(&self) -> (bool, decklight_outputs::FrameRate) {
        let s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        (s.timecode_enabled, s.timecode.generator.rate)
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    pub fn show_data(&self) -> ShowData {
        self.core()
            .with(|s| ShowData {
                offsets: s.offsets.clone(),
                triggers: s.triggers.clone(),
            })
            .unwrap_or_default()
    }

    /// Hinweis ins Monitoring schreiben.
    pub fn warn(&self, level: decklight_core::Level, text: impl Into<String>) {
        let text = text.into();
        self.core().with(|s| s.monitor.warn(level, text));
    }

    /// Ordner für Show-Dateien und Sicherungen.
    pub fn set_library(&self, dir: impl Into<PathBuf>) {
        if let Ok(mut l) = self.library.lock() {
            *l = Some(Arc::new(library::Library::new(dir)));
        }
    }

    pub fn library(&self) -> Option<Arc<library::Library>> {
        self.library.lock().ok().and_then(|l| l.clone())
    }

    pub fn show_name(&self) -> String {
        self.show_name.lock().map(|n| n.clone()).unwrap_or_default()
    }

    pub fn set_show_name(&self, name: &str) {
        if let Ok(mut n) = self.show_name.lock() {
            *n = name.to_string();
        }
    }

    /// Laufende Show als Datei (mit Ausgangs-Einstellungen).
    pub fn show_file(&self) -> library::ShowFile {
        library::ShowFile::new(
            &self.show_name(),
            self.show_data(),
            Some(library::OutputSettings::from_settings(&self.settings())),
        )
    }

    /// Show-Daten ersetzen; mit Ausgangs-Einstellungen werden diese übernommen.
    pub async fn load_show(self: &Arc<Self>, file: library::ShowFile) -> Result<()> {
        file.check()?;
        if let Some(out) = &file.outputs {
            self.update_settings(out.apply_to(&self.settings())).await?;
        }
        let show = file.show;
        self.core().with(move |s| {
            s.offsets = show.offsets;
            s.triggers = show.triggers;
        });
        self.set_show_name(&file.name);
        Ok(())
    }

    /// Ereignisse des aktuellen Kerns als fertiges JSON an alle Clients.
    fn forward_events(self: &Arc<Self>) {
        let mut rx = self.core().subscribe();
        let tx = self.events.clone();
        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if let Ok(json) = serde_json::to_string(&Tagged {
                            kind: "event",
                            body: &ev,
                        }) {
                            let _ = tx.send(json.into());
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return,
                }
            }
        });
        if let Ok(mut f) = self.forwarder.lock() {
            if let Some(old) = f.replace(task) {
                old.abort();
            }
        }
    }

    /// Neue Einstellungen übernehmen. Netz-Änderungen starten den Kern neu,
    /// Show-Daten bleiben erhalten.
    pub async fn update_settings(self: &Arc<Self>, new: Settings) -> Result<()> {
        let _guard = self.restart.lock().await;
        let old = self.settings();
        if old.needs_restart(&new) {
            new.link_config()?;
            let show = self.show_data();
            let master_mode = new.master_mode;
            // Platzhalter einsetzen, alten Kern freigeben, dann neu starten.
            let previous = {
                let placeholder = start_core(&passive_placeholder(), ShowData::default()).await?;
                let mut c = self.core.write().map_err(|_| anyhow::anyhow!("Sperre"))?;
                std::mem::replace(&mut *c, placeholder)
            };
            wait_until_unique(&previous).await;
            drop(previous);
            tokio::time::sleep(Duration::from_millis(100)).await;
            let core = match start_core(&new, show.clone()).await {
                Ok(c) => c,
                Err(e) => {
                    // Mit den alten Einstellungen weiterlaufen.
                    let c = start_core(&old, show).await?;
                    self.swap_core(c);
                    return Err(e);
                }
            };
            core.with(|s| s.engine.set_master_mode(master_mode));
            self.swap_core(core);
        } else {
            let core = self.core();
            core.set_outputs(new.timecode_config(), new.actions.clone());
            core.with(|s| s.engine.set_master_mode(new.master_mode));
        }
        if let Ok(mut s) = self.settings.lock() {
            *s = new;
        }
        Ok(())
    }

    fn swap_core(self: &Arc<Self>, core: Arc<Core>) {
        if let Ok(mut c) = self.core.write() {
            *c = core;
        }
        self.forward_events();
    }
}

/// Kern ohne Netzwerkzugriff für die kurze Zeit eines Neustarts.
fn passive_placeholder() -> Settings {
    let mut s = Settings::default();
    s.network.passive = true;
    s.network.port_base = 0;
    s.timecode_enabled = false;
    s
}

async fn wait_until_unique(core: &Arc<Core>) {
    for _ in 0..50 {
        if Arc::strong_count(core) == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn start_core(settings: &Settings, show: ShowData) -> Result<Arc<Core>> {
    let mut cfg = settings.core_config()?;
    if settings.network.port_base == 0 {
        cfg.link.ports = decklight_prolink::PortConfig {
            announce: 0,
            beat: 0,
            status: 0,
        };
    }
    cfg.offsets = show.offsets;
    cfg.triggers = show.triggers;
    let mut last = None;
    // Nach einem Neustart sind die alten Ports eventuell noch kurz belegt.
    for _ in 0..10 {
        match Core::start(cfg.clone()).await {
            Ok(c) => return Ok(Arc::new(c)),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    Err(last.map_or_else(|| anyhow::anyhow!("Start fehlgeschlagen"), Into::into))
        .context("PRO-DJ-LINK-Ports konnten nicht geöffnet werden")
}

#[derive(Serialize)]
struct Tagged<'a, T: Serialize> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(flatten)]
    body: T,
}

fn tagged<T: Serialize>(kind: &str, body: T) -> String {
    serde_json::to_string(&Tagged { kind, body }).unwrap_or_default()
}

/// Eingebaute Oberfläche: Pfad (ohne führenden Schrägstrich) → Inhalt und MIME-Typ.
/// Die Desktop-App reicht hier ihre eigenen Dateien durch, damit Handy und
/// Tablet `/monitor` direkt vom DeckLight-Rechner laden können.
#[derive(Clone)]
pub struct UiAssets(pub Arc<AssetFn>);

/// Liefert zu einem Pfad Inhalt und MIME-Typ.
pub type AssetFn = dyn Fn(&str) -> Option<(Vec<u8>, String)> + Send + Sync;

impl std::fmt::Debug for UiAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UiAssets")
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    /// Gebaute Oberfläche (`ui/dist`). Ohne: nur API und Hinweisseite.
    pub ui_dir: Option<PathBuf>,
    /// Eingebaute Oberfläche, falls kein `ui_dir` angegeben ist.
    pub ui_assets: Option<UiAssets>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT)),
            ui_dir: None,
            ui_assets: None,
        }
    }
}

pub struct Server {
    pub addr: SocketAddr,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    pub async fn start(app: Arc<App>, cfg: ServerConfig) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind(cfg.bind)
            .await
            .with_context(|| format!("Webserver: {} belegt", cfg.bind))?;
        let addr = listener.local_addr()?;
        let router = match (cfg.ui_dir, cfg.ui_assets) {
            (None, Some(assets)) => router_embedded(app, assets),
            (dir, _) => router(app, dir),
        };
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            {
                tracing::error!(error = %e, "Webserver beendet");
            }
        });
        tracing::info!(%addr, "Webserver läuft");
        Ok(Self { addr, task })
    }
}

pub fn router(app: Arc<App>, ui_dir: Option<PathBuf>) -> Router {
    let api = api_router(app);
    match ui_dir.filter(|d| d.join("index.html").is_file()) {
        Some(dir) => {
            let index = dir.join("index.html");
            api.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
        }
        None => api.fallback(no_ui),
    }
}

/// Wie [`router`], aber mit eingebauter Oberfläche.
pub fn router_embedded(app: Arc<App>, assets: UiAssets) -> Router {
    api_router(app).fallback(move |uri: axum::http::Uri| {
        let assets = assets.clone();
        async move { embedded(&assets, uri.path()) }
    })
}

/// Eine Datei der eingebauten Oberfläche. Unbekannte Pfade ohne Dateiendung
/// (`/monitor`, `/overlay`) bekommen die Startseite, die selbst weiterleitet.
fn embedded(assets: &UiAssets, path: &str) -> Response {
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let hit = (assets.0)(path).or_else(|| {
        let last = path.rsplit('/').next().unwrap_or(path);
        (!last.contains('.'))
            .then(|| (assets.0)("index.html"))
            .flatten()
    });
    match hit {
        Some((bytes, mime)) => {
            // Gebaute Dateien tragen einen Hash im Namen und ändern sich nie.
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, cache.to_string()),
                ],
                bytes,
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "Nicht gefunden").into_response(),
    }
}

fn api_router(app: Arc<App>) -> Router {
    Router::new()
        .route("/ws", get(ws))
        .route("/api/state", get(api_state))
        .route("/api/show", get(api_show))
        .route("/api/monitor", get(api_monitor))
        .route("/api/track/{key}", get(api_track))
        .route("/api/artwork/{key}", get(api_artwork))
        .route("/api/command", post(api_command))
        .route("/api/settings", get(api_settings))
        .route("/api/interfaces", get(api_interfaces))
        .route("/api/midi-ports", get(api_midi_ports))
        .route("/api/history.csv", get(csv_history))
        .route("/api/log.csv", get(csv_log))
        .route("/api/offsets.csv", get(csv_offsets))
        .route("/api/shows", get(api_shows))
        .route("/api/shows/export", get(api_show_export))
        .route("/api/access", get(api_access))
        .with_state(app)
        .layer(desktop_cors())
}

/// Die Desktop-App lädt die Oberfläche von `tauri://localhost` und spricht von
/// dort mit diesem Server; andere fremde Seiten bekommen keine CORS-Freigabe.
fn desktop_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            matches!(
                origin.as_bytes(),
                b"tauri://localhost" | b"http://tauri.localhost" | b"https://tauri.localhost"
            )
        }))
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([header::CONTENT_TYPE])
}

async fn no_ui() -> Html<&'static str> {
    Html(
        "<!doctype html><meta charset=utf-8><title>DeckLight</title>\
         <body style=\"background:#0b0d10;color:#e6e8eb;font-family:system-ui;padding:2rem\">\
         <h1>DeckLight</h1><p>Die Oberfläche ist nicht gebaut (<code>cd ui &amp;&amp; npm run build</code>). \
         Die Schnittstelle läuft: <a style=color:#7cc4ff href=/api/state>/api/state</a>, <code>/ws</code>.</p>",
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Denied,
    ReadOnly,
    Control,
}

#[derive(Debug, Default, Deserialize)]
struct PinQuery {
    pin: Option<String>,
}

fn access(app: &App, peer: SocketAddr, pin: Option<&str>) -> Access {
    if peer.ip().is_loopback() {
        return Access::Control;
    }
    let web = app.settings().web;
    if let Some(expected) = web.pin.as_deref().filter(|p| !p.is_empty()) {
        if pin != Some(expected) {
            return Access::Denied;
        }
    }
    if web.allow_lan_control {
        Access::Control
    } else {
        Access::ReadOnly
    }
}

fn pin_of<'a>(q: &'a PinQuery, headers: &'a HeaderMap) -> Option<&'a str> {
    q.pin
        .as_deref()
        .or_else(|| headers.get("x-decklight-pin").and_then(|v| v.to_str().ok()))
}

/// Abgelehnter Zugriff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rejected {
    NeedPin,
    ReadOnly,
}

impl IntoResponse for Rejected {
    fn into_response(self) -> Response {
        match self {
            Rejected::NeedPin => (StatusCode::UNAUTHORIZED, "PIN erforderlich").into_response(),
            Rejected::ReadOnly => (
                StatusCode::FORBIDDEN,
                "Nur lesend: Befehle gehen nur vom DeckLight-Rechner",
            )
                .into_response(),
        }
    }
}

/// Prüft den Zugriff.
fn guard(
    app: &App,
    peer: SocketAddr,
    q: &PinQuery,
    headers: &HeaderMap,
    need_control: bool,
) -> Result<Access, Rejected> {
    match access(app, peer, pin_of(q, headers)) {
        Access::Denied => Err(Rejected::NeedPin),
        Access::ReadOnly if need_control => Err(Rejected::ReadOnly),
        a => Ok(a),
    }
}

type Peer = ConnectInfo<SocketAddr>;

async fn ws(
    upgrade: WebSocketUpgrade,
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    let access = match guard(&app, peer, &q, &headers, false) {
        Ok(a) => a,
        Err(r) => return r.into_response(),
    };
    upgrade.on_upgrade(move |socket| client(socket, app, access == Access::ReadOnly))
}

#[derive(Serialize)]
struct NamedShow<'a> {
    name: &'a str,
    #[serde(flatten)]
    show: view::ShowView,
}

#[derive(Serialize)]
struct Hello {
    version: &'static str,
    readonly: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<Settings>,
}

async fn client(mut socket: WebSocket, app: Arc<App>, readonly: bool) {
    let hello = tagged(
        "hello",
        Hello {
            version: env!("CARGO_PKG_VERSION"),
            readonly,
            settings: (!readonly).then(|| app.settings()),
        },
    );
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }
    let mut events = app.events.subscribe();
    // 40 Frames/s; verspätete Ticks werden nachgeholt, damit es im Mittel
    // auch bei grobem Timer (Windows) über 30/s bleibt.
    let mut frame = tokio::time::interval(Duration::from_millis(25));
    frame.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
    let mut slow = tokio::time::interval(Duration::from_millis(500));
    slow.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_show = String::new();
    let mut last_settings = String::new();

    loop {
        let msg: Option<String> = tokio::select! {
            _ = frame.tick() => {
                let core = app.core();
                let (enabled, rate) = app.timecode_view_settings();
                let t = core.now();
                core.with(|s| tagged("state", view::state(s, t, enabled, rate)))
            }
            _ = slow.tick() => {
                let core = app.core();
                let mut out = core.with(|s| tagged("monitor", view::monitor(s)));
                let name = app.show_name();
                let show = core
                    .with(|s| tagged("show", NamedShow { name: &name, show: view::show(s) }))
                    .unwrap_or_default();
                if show != last_show {
                    if socket.send(Message::Text(show.clone().into())).await.is_err() {
                        return;
                    }
                    last_show = show;
                }
                if !readonly {
                    let settings = tagged("settings", app.settings());
                    if settings != last_settings {
                        last_settings = settings.clone();
                        if socket.send(Message::Text(settings.into())).await.is_err() {
                            return;
                        }
                    }
                }
                out.take()
            }
            ev = events.recv() => match ev {
                Ok(json) => Some(json.to_string()),
                Err(broadcast::error::RecvError::Lagged(_)) => None,
                Err(_) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => None,
            },
        };
        if let Some(text) = msg {
            if socket.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }
}

macro_rules! guarded {
    ($app:expr, $peer:expr, $q:expr, $headers:expr, $control:expr) => {
        if let Err(r) = guard(&$app, $peer, &$q, &$headers, $control) {
            return r.into_response();
        }
    };
}

async fn api_state(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, false);
    let core = app.core();
    let t = core.now();
    let (enabled, rate) = app.timecode_view_settings();
    Json(core.with(|s| view::state(s, t, enabled, rate))).into_response()
}

async fn api_show(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, false);
    Json(app.core().with(|s| view::show(s))).into_response()
}

async fn api_monitor(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, false);
    Json(app.core().with(|s| view::monitor(s))).into_response()
}

async fn api_track(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    guarded!(app, peer, q, headers, false);
    match app
        .core()
        .with(|s| view::track_detail(s, &TrackKey(key)))
        .flatten()
    {
        Some(d) => Json(d).into_response(),
        None => (StatusCode::NOT_FOUND, "Track unbekannt").into_response(),
    }
}

async fn api_artwork(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    guarded!(app, peer, q, headers, false);
    let art = app
        .core()
        .with(|s| s.tracks.get(&TrackKey(key)).and_then(|d| d.artwork.clone()))
        .flatten();
    match art {
        Some(bytes) => {
            let mime = if bytes.starts_with(b"\x89PNG") {
                "image/png"
            } else {
                "image/jpeg"
            };
            (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, "max-age=3600"),
                ],
                bytes,
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "kein Cover").into_response(),
    }
}

async fn api_command(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
    Json(cmd): Json<Command>,
) -> Response {
    guarded!(app, peer, q, headers, true);
    let reply = match cmd {
        Command::UpdateSettings { settings } => match app.update_settings(*settings).await {
            Ok(()) => Reply::ok(),
            Err(e) => Reply::err(format!("{e:#}")),
        },
        c @ (Command::ShowSave { .. }
        | Command::ShowOpen { .. }
        | Command::ShowNew { .. }
        | Command::ShowImport { .. }
        | Command::BackupRestore { .. }) => match show_command(&app, c).await {
            Ok(()) => Reply::ok(),
            Err(e) => Reply::err(format!("{e:#}")),
        },
        other => {
            let rate = app.settings().timecode.generator.rate;
            command::apply(&app.core(), other, rate)
        }
    };
    let status = if reply.ok {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(reply)).into_response()
}

async fn show_command(app: &Arc<App>, cmd: Command) -> Result<()> {
    let lib = || {
        app.library()
            .context("Kein Ordner für Show-Dateien eingestellt")
    };
    match cmd {
        Command::ShowSave { name } => {
            let name = library::clean_name(&name.unwrap_or_else(|| app.show_name()))?;
            app.set_show_name(&name);
            lib()?.save(&app.show_file())?;
        }
        Command::ShowOpen { name } => {
            let file = lib()?.open(&name)?;
            app.load_show(file).await?;
        }
        Command::ShowNew { name } => {
            let name = library::clean_name(&name)?;
            app.load_show(library::ShowFile::new(&name, ShowData::default(), None))
                .await?;
        }
        Command::ShowImport { file } => app.load_show(*file).await?,
        Command::BackupRestore { file } => {
            let show = lib()?.open_backup(&file)?;
            let name = app.show_name();
            app.load_show(library::ShowFile::new(&name, show, None))
                .await?;
        }
        _ => anyhow::bail!("kein Show-Befehl"),
    }
    Ok(())
}

#[derive(Serialize)]
struct ShowsView {
    current: String,
    shows: Vec<library::ShowInfo>,
    backups: Vec<library::ShowInfo>,
}

async fn api_shows(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, true);
    let lib = app.library();
    Json(ShowsView {
        current: app.show_name(),
        shows: lib.as_ref().map(|l| l.list()).unwrap_or_default(),
        backups: lib.as_ref().map(|l| l.backups()).unwrap_or_default(),
    })
    .into_response()
}

async fn api_show_export(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, true);
    let file = app.show_file();
    let name = library::clean_name(&file.name).unwrap_or_else(|_| "show".into());
    let body = serde_json::to_string_pretty(&file).unwrap_or_default();
    (
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}.{}\"",
                    name.replace(
                        |c: char| !c.is_ascii_alphanumeric() && c != '-' && c != ' ',
                        "_"
                    ),
                    library::EXTENSION
                ),
            ),
        ],
        body,
    )
        .into_response()
}

async fn api_settings(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, true);
    Json(app.settings()).into_response()
}

/// Wie Handy und Tablet den DeckLight-Rechner erreichen.
#[derive(Serialize)]
struct AccessView {
    port: u16,
    /// Adressen im Netz, private zuerst; jede mit `/monitor` und ggf. PIN.
    urls: Vec<AccessUrl>,
    pin: Option<String>,
    allow_lan_control: bool,
}

#[derive(Serialize)]
struct AccessUrl {
    iface: String,
    ip: String,
    monitor: String,
}

async fn api_access(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, true);
    let web = app.settings().web;
    let port = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.rsplit_once(':'))
        .and_then(|(_, p)| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let pin = web.pin.filter(|p| !p.is_empty());
    let urls = access_urls(
        decklight_prolink::interfaces()
            .unwrap_or_default()
            .into_iter()
            .filter(|i| !i.loopback)
            .map(|i| (i.name, i.ip))
            .collect(),
        port,
        pin.as_deref(),
    );
    Json(AccessView {
        port,
        urls,
        pin,
        allow_lan_control: web.allow_lan_control,
    })
    .into_response()
}

/// Handy-Adressen: WLAN-/Heimnetz-Adressen zuerst, Link-Local (169.254, meist
/// das CDJ-Kabel) zuletzt, weil das Handy dort fast nie hängt.
fn access_urls(
    ifaces: Vec<(String, std::net::Ipv4Addr)>,
    port: u16,
    pin: Option<&str>,
) -> Vec<AccessUrl> {
    let mut list: Vec<_> = ifaces
        .into_iter()
        .filter(|(_, ip)| !ip.is_loopback() && !ip.is_unspecified())
        .collect();
    list.sort_by_key(|(_, ip)| (ip.is_link_local(), !ip.is_private()));
    list.into_iter()
        .map(|(iface, ip)| {
            let mut monitor = format!("http://{ip}:{port}/monitor");
            if let Some(p) = pin {
                monitor.push_str("?pin=");
                monitor.extend(p.bytes().map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                }));
            }
            AccessUrl {
                iface,
                ip: ip.to_string(),
                monitor,
            }
        })
        .collect()
}

#[derive(Serialize)]
struct InterfaceView {
    name: String,
    ip: String,
    broadcast: String,
}

async fn api_interfaces(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, true);
    let list: Vec<InterfaceView> = decklight_prolink::interfaces()
        .unwrap_or_default()
        .into_iter()
        .map(|i| InterfaceView {
            name: i.name,
            ip: i.ip.to_string(),
            broadcast: i.broadcast.to_string(),
        })
        .collect();
    Json(list).into_response()
}

async fn api_midi_ports(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, true);
    let ports = decklight_outputs::midi::output_ports().unwrap_or_default();
    Json(ports).into_response()
}

fn csv(name: &str, body: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        body,
    )
        .into_response()
}

async fn csv_history(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, false);
    csv(
        "decklight-history.csv",
        app.core()
            .with(|s| s.monitor.history_csv())
            .unwrap_or_default(),
    )
}

async fn csv_log(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, false);
    csv(
        "decklight-log.csv",
        app.core()
            .with(|s| s.trigger_log.to_csv())
            .unwrap_or_default(),
    )
}

async fn csv_offsets(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): Peer,
    Query(q): Query<PinQuery>,
    headers: HeaderMap,
) -> Response {
    guarded!(app, peer, q, headers, false);
    let rate = app.settings().timecode.generator.rate;
    csv(
        "decklight-offsets.csv",
        app.core()
            .with(|s| s.offsets.to_csv(rate))
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_ui_serves_files_and_falls_back_to_index() {
        let assets = UiAssets(Arc::new(|p: &str| match p {
            "index.html" => Some((b"<html>".to_vec(), "text/html".into())),
            "assets/app-1.js" => Some((b"js".to_vec(), "text/javascript".into())),
            _ => None,
        }));
        let ok = |path: &str| embedded(&assets, path).status();
        assert_eq!(ok("/"), StatusCode::OK);
        assert_eq!(ok("/monitor"), StatusCode::OK);
        assert_eq!(ok("/assets/app-1.js"), StatusCode::OK);
        assert_eq!(ok("/assets/fehlt.js"), StatusCode::NOT_FOUND);
        let r = embedded(&assets, "/assets/app-1.js");
        assert_eq!(r.headers()[header::CONTENT_TYPE], "text/javascript");
        assert!(r.headers()[header::CACHE_CONTROL]
            .to_str()
            .unwrap()
            .contains("immutable"));
    }

    #[test]
    fn access_urls_prefer_wifi_and_carry_pin() {
        let urls = access_urls(
            vec![
                ("eth0".into(), "169.254.10.2".parse().unwrap()),
                ("wlan0".into(), "192.168.178.23".parse().unwrap()),
            ],
            8750,
            Some("12 34"),
        );
        assert_eq!(urls[0].iface, "wlan0");
        assert_eq!(
            urls[0].monitor,
            "http://192.168.178.23:8750/monitor?pin=12%2034"
        );
        assert_eq!(urls[1].ip, "169.254.10.2");
        let plain = access_urls(
            vec![("en0".into(), "10.0.0.5".parse().unwrap())],
            8750,
            None,
        );
        assert_eq!(plain[0].monitor, "http://10.0.0.5:8750/monitor");
    }

    #[tokio::test]
    async fn pin_and_read_only_access() {
        let mut settings = Settings::default();
        settings.network.passive = true;
        settings.network.port_base = 0;
        settings.timecode_enabled = false;
        let app = App::start(settings.clone(), ShowData::default())
            .await
            .unwrap();
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let lan: SocketAddr = "192.168.1.20:5000".parse().unwrap();
        assert_eq!(access(&app, local, None), Access::Control);
        assert_eq!(access(&app, lan, None), Access::ReadOnly);

        let mut s = settings;
        s.web.pin = Some("4711".into());
        app.update_settings(s.clone()).await.unwrap();
        assert_eq!(access(&app, lan, None), Access::Denied);
        assert_eq!(access(&app, lan, Some("0000")), Access::Denied);
        assert_eq!(access(&app, lan, Some("4711")), Access::ReadOnly);
        assert_eq!(access(&app, local, None), Access::Control);

        s.web.allow_lan_control = true;
        app.update_settings(s).await.unwrap();
        assert_eq!(access(&app, lan, Some("4711")), Access::Control);
    }
}
