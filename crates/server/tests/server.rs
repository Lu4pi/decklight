//! Webserver gegen simulierte CDJs: WebSocket-Frames (≥ 30/s), Titel, Wellenform,
//! Cover, Befehle, Not-Aus und Neustart mit neuen Netz-Einstellungen.

use decklight_library::dbserver::server::DbServer;
use decklight_server::settings::Settings;
use decklight_server::{App, Server, ServerConfig, ShowData};
use decklight_simulator::{Scenario, SimLibrary, Simulator, Target};
use futures_util::StreamExt;
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

async fn http(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.unwrap();
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let status: u16 = head.split(' ').nth(1).unwrap().parse().unwrap();
    assert!(
        !head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked"),
        "{head}"
    );
    (status, buf[split + 4..].to_vec())
}

async fn json(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> (u16, Value) {
    let (status, bytes) = http(addr, method, path, body).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn websocket_state_commands_and_restart() {
    let scenario = Scenario::demo();
    let db = DbServer::start(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        Arc::new(SimLibrary::from_scenario(&scenario)),
    )
    .await
    .unwrap();
    let mut settings = Settings::default();
    settings.network.passive = true;
    settings.network.port_base = 0;
    settings.network.db_port = db.query_port;
    settings.timecode_enabled = false;
    let app = App::start(settings.clone(), ShowData::default())
        .await
        .unwrap();
    let server = Server::start(
        app.clone(),
        ServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            ui_dir: None,
        },
    )
    .await
    .unwrap();
    let addr = server.addr;

    let target = Target {
        ip: Ipv4Addr::LOCALHOST,
        ports: app.core().link().local_ports(),
    };
    let sim = tokio::spawn(decklight_simulator::run(
        Simulator::new(scenario.clone(), Ipv4Addr::LOCALHOST),
        target,
        Duration::from_millis(5),
    ));

    // WebSocket: Begrüßung, dann Zustand mit mindestens 30 Frames pro Sekunde.
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    let first: Value = match ws.next().await.unwrap().unwrap() {
        Message::Text(t) => serde_json::from_str(&t).unwrap(),
        other => panic!("{other:?}"),
    };
    assert_eq!(first["type"], "hello");
    assert_eq!(first["readonly"], false);

    let mut frames = 0;
    let mut title = None;
    let mut saw_show = false;
    let start = Instant::now();
    let mut measured = None;
    while start.elapsed() < Duration::from_secs(6) {
        let Some(Ok(Message::Text(t))) = ws.next().await else {
            continue;
        };
        let v: Value = serde_json::from_str(&t).unwrap();
        match v["type"].as_str() {
            Some("state") => {
                frames += 1;
                if let Some(t) = v["decks"][0]["info"]["title"].as_str() {
                    if v["decks"][0]["info"]["loaded"] == true {
                        title = Some(t.to_string());
                    }
                }
            }
            Some("show") => saw_show = true,
            _ => {}
        }
        if start.elapsed() >= Duration::from_secs(2) && measured.is_none() {
            measured = Some(frames);
        }
        if title.is_some() && saw_show && measured.is_some() {
            break;
        }
    }
    let per_s = f64::from(measured.unwrap()) / 2.0;
    println!("{per_s:.1} Zustands-Frames pro Sekunde");
    assert!(per_s >= 30.0, "nur {per_s:.1} Frames/s");
    assert_eq!(title.as_deref(), Some("Midnight Signal"));
    assert!(saw_show);

    // Track-Details: Wellenform, Beats, Rekordbox-Cues, Vorlagen; Cover.
    let (status, show) = json(addr, "GET", "/api/show", None).await;
    assert_eq!(status, 200);
    let key = show["tracks"][0]["key"].as_str().unwrap().to_string();
    let (status, detail) = json(addr, "GET", &format!("/api/track/{}", encode(&key)), None).await;
    assert_eq!(status, 200, "{key}");
    assert!(detail["waveform"]["columns"].as_u64().unwrap() > 10_000);
    assert!(detail["beats"].as_array().unwrap().len() > 500);
    assert_eq!(detail["cues"].as_array().unwrap().len(), 5);
    let (status, art) = http(addr, "GET", &format!("/api/artwork/{}", encode(&key)), None).await;
    assert_eq!(status, 200);
    assert!(art.starts_with(b"\x89PNG"));

    // Befehle: Cue anlegen, Offset setzen, Not-Aus.
    let cue = format!(
        r#"{{"cmd":"add_cue","key":{},"cue":{{"id":0,"name":"Licht","position":{{"type":"ms","ms":30000}},"actions":[{{"type":"osc","command":"Go+ Exec 5"}}]}}}}"#,
        serde_json::to_string(&key).unwrap()
    );
    let (status, reply) = json(addr, "POST", "/api/command", Some(&cue)).await;
    assert_eq!(status, 200, "{reply}");
    let id = reply["id"].as_u64().unwrap();
    let offset = format!(
        r#"{{"cmd":"set_offset","key":{},"offset_ms":7200000}}"#,
        serde_json::to_string(&key).unwrap()
    );
    assert_eq!(
        json(addr, "POST", "/api/command", Some(&offset)).await.0,
        200
    );
    let (status, _) = json(addr, "POST", "/api/command", Some(r#"{"cmd":"panic"}"#)).await;
    assert_eq!(status, 200);
    let (_, state) = json(addr, "GET", "/api/state", None).await;
    assert_eq!(state["armed"], false);
    let (status, reply) = json(
        addr,
        "POST",
        "/api/command",
        Some(r#"{"cmd":"remove","id":99999}"#),
    )
    .await;
    assert_eq!(status, 400);
    assert!(reply["error"].as_str().is_some());

    // CSV-Exporte.
    let (status, csv) = http(addr, "GET", "/api/offsets.csv", None).await;
    assert_eq!(status, 200);
    assert!(String::from_utf8(csv).unwrap().contains("02:00:00:00"));
    assert_eq!(http(addr, "GET", "/api/history.csv", None).await.0, 200);

    // Netz-Einstellungen ändern: Kern startet neu, Cues und Offsets bleiben.
    let old_ports = app.core().link().local_ports();
    let mut new = settings.clone();
    new.network.player = 9;
    let body = serde_json::json!({"cmd": "update_settings", "settings": new}).to_string();
    let (status, reply) = json(addr, "POST", "/api/command", Some(&body)).await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(app.settings().network.player, 9);
    let data = app.show_data();
    assert!(data.triggers.cue(id as u32).is_some());
    assert!(!data.triggers.armed, "Not-Aus bleibt nach Neustart");
    assert_eq!(
        data.offsets
            .get(&decklight_show::TrackKey(key.clone()))
            .unwrap()
            .offset_ms,
        7_200_000.0
    );
    assert_ne!(app.core().link().local_ports(), old_ports);

    sim.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn show_files_save_open_new_import_and_backups() {
    let dir = std::env::temp_dir().join(format!("decklight-shows-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut settings = Settings::default();
    settings.network.passive = true;
    settings.network.port_base = 0;
    settings.timecode_enabled = false;
    let app = App::start(settings, ShowData::default()).await.unwrap();
    app.set_library(&dir);
    app.set_show_name("Probe");
    let server = Server::start(
        app.clone(),
        ServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            ui_dir: None,
        },
    )
    .await
    .unwrap();
    let addr = server.addr;
    let cmd = |body: String| async move { json(addr, "POST", "/api/command", Some(&body)).await };

    let cue = r#"{"cmd":"add_cue","key":"T","cue":{"id":0,"name":"Drop","position":{"type":"ms","ms":1000},"actions":[]}}"#;
    assert_eq!(cmd(cue.into()).await.0, 200);
    let (status, reply) = cmd(r#"{"cmd":"show_save","name":"Club Night"}"#.into()).await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(app.show_name(), "Club Night");

    // Neue, leere Show; danach die gespeicherte wieder öffnen.
    assert_eq!(
        cmd(r#"{"cmd":"show_new","name":"Leer"}"#.into()).await.0,
        200
    );
    assert_eq!(app.show_data().triggers.tracks.len(), 0);
    let (_, shows) = json(addr, "GET", "/api/shows", None).await;
    assert_eq!(shows["current"], "Leer");
    assert_eq!(shows["shows"][0]["name"], "Club Night");
    assert_eq!(shows["shows"][0]["cues"], 1);
    assert_eq!(
        cmd(r#"{"cmd":"show_open","name":"Club Night"}"#.into())
            .await
            .0,
        200
    );
    assert_eq!(app.show_data().triggers.tracks.len(), 1);
    assert_eq!(
        cmd(r#"{"cmd":"show_open","name":"../settings"}"#.into())
            .await
            .0,
        400
    );

    // Export und Import über die Datei.
    let (status, export) = json(addr, "GET", "/api/shows/export", None).await;
    assert_eq!(status, 200);
    assert_eq!(export["format"], "decklight-show");
    assert_eq!(
        cmd(r#"{"cmd":"show_new","name":"Leer"}"#.into()).await.0,
        200
    );
    let mut file = export.clone();
    file["name"] = "Importiert".into();
    let body = serde_json::json!({"cmd": "show_import", "file": file}).to_string();
    assert_eq!(cmd(body).await.0, 200);
    assert_eq!(app.show_name(), "Importiert");
    assert_eq!(app.show_data().triggers.tracks.len(), 1);
    file["format"] = "fremd".into();
    let body = serde_json::json!({"cmd": "show_import", "file": file}).to_string();
    assert_eq!(cmd(body).await.0, 400);

    // Sicherung zurückholen.
    let lib = app.library().unwrap();
    lib.backup(&app.show_data(), Duration::ZERO).unwrap();
    assert_eq!(
        cmd(r#"{"cmd":"show_new","name":"Leer"}"#.into()).await.0,
        200
    );
    let backup = lib.backups()[0].file.clone();
    let body = serde_json::json!({"cmd": "backup_restore", "file": backup}).to_string();
    assert_eq!(cmd(body).await.0, 200);
    assert_eq!(app.show_data().triggers.tracks.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}
