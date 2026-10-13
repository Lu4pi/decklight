//! Track-Daten per NFS vom „Player“ (Testserver über den Demo-Export).

use decklight_library::nfs::{server, NfsClient};
use decklight_library::MediaCache;
use std::path::Path;

fn demo() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rekordbox-demo")
}

#[tokio::test]
async fn reads_files_with_utf16_names() {
    let srv = server::serve(vec![("/C/".into(), demo())]).await.unwrap();
    let ip = "127.0.0.1".parse().unwrap();
    let mut c = NfsClient::mount_via(ip, srv.portmap_port, "/C/")
        .await
        .unwrap();
    let pdb = c.read_file("/PIONEER/rekordbox/export.pdb").await.unwrap();
    assert_eq!(
        pdb,
        std::fs::read(demo().join("PIONEER/rekordbox/export.pdb")).unwrap()
    );
    assert!(c.read_file("/PIONEER/fehlt.txt").await.is_err());
    assert!(NfsClient::mount_via(ip, srv.portmap_port, "/B/")
        .await
        .is_err());
}

#[tokio::test]
async fn media_cache_loads_waveform_and_grid() {
    let srv = server::serve(vec![("/C/".into(), demo())]).await.unwrap();
    let dir = std::env::temp_dir().join(format!("decklight-nfs-test-{}", std::process::id()));
    let cache = MediaCache::new(dir.clone());
    let ip = "127.0.0.1".parse().unwrap();
    let t = cache.track_via(ip, srv.portmap_port, 3, 1).await.unwrap();
    assert_eq!(t.metadata.title, "Demo Track 1");
    assert_eq!(t.beat_grid.len(), 368);
    assert!(t.waveform.is_some());
    // Zweiter Track: export.pdb kommt aus dem Spiegel.
    let t2 = cache.track_via(ip, srv.portmap_port, 3, 2).await.unwrap();
    assert_eq!(t2.metadata.bpm, Some(120.0));
    assert!(cache.track_via(ip, srv.portmap_port, 3, 99).await.is_err());
    let _ = std::fs::remove_dir_all(dir);
}
