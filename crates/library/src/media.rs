//! Track-Daten direkt vom Medium im Player (NFS), wenn der dbserver nichts
//! liefert. `export.pdb` und die benötigten ANLZ-Dateien landen in einem
//! Spiegelverzeichnis und werden dann wie ein normaler Export gelesen.

use crate::export::Export;
use crate::nfs::{FileAttr, NfsClient, NfsError};
use crate::track::TrackData;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PDB: &str = "/PIONEER/rekordbox/export.pdb";

type Mirror = (FileAttr, Arc<Export>);

pub struct MediaCache {
    dir: PathBuf,
    mirrors: Mutex<HashMap<(IpAddr, u8), Mirror>>,
    /// Pro Medium nur ein Abgleich gleichzeitig.
    busy: tokio::sync::Mutex<()>,
}

impl MediaCache {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            mirrors: Mutex::default(),
            busy: tokio::sync::Mutex::new(()),
        }
    }

    fn root(&self, ip: IpAddr, slot: u8) -> PathBuf {
        self.dir
            .join(format!("{ip}-{slot}").replace([':', '%'], "_"))
    }

    /// Track `id` aus dem Slot `slot` des Players `ip`.
    pub async fn track(&self, ip: IpAddr, slot: u8, id: u32) -> Result<TrackData, NfsError> {
        self.track_via(ip, crate::nfs::PORTMAP_PORT, slot, id).await
    }

    pub async fn track_via(
        &self,
        ip: IpAddr,
        portmap: u16,
        slot: u8,
        id: u32,
    ) -> Result<TrackData, NfsError> {
        let _guard = self.busy.lock().await;
        let export = crate::nfs::export_path(slot).ok_or(NfsError::NoSlot(slot))?;
        let mut c = NfsClient::mount_via(ip, portmap, export).await?;
        let at = c.stat(PDB).await?;
        let root = self.root(ip, slot);
        let cached = self.mirrors.lock().ok().and_then(|m| {
            m.get(&(ip, slot))
                .filter(|(a, _)| *a == at)
                .map(|(_, e)| e.clone())
        });
        let ex = match cached {
            Some(ex) => ex,
            None => {
                tracing::info!(%ip, slot, bytes = at.size, "Lese export.pdb vom Player");
                let data = c.read_file(PDB).await?;
                // Anderer Stick oder neuer Export: alten Spiegel verwerfen.
                let _ = std::fs::remove_dir_all(&root);
                write(&root, PDB, &data)?;
                let ex = Arc::new(
                    Export::open(&root)
                        .map_err(|e| NfsError::Io(std::io::Error::other(e.to_string())))?,
                );
                if let Ok(mut m) = self.mirrors.lock() {
                    m.insert((ip, slot), (at, ex.clone()));
                }
                ex
            }
        };
        let Some(t) = ex.db.tracks.get(&id) else {
            return Err(NfsError::Status(2, format!("Track {id} im Export")));
        };
        let mut wanted = Vec::new();
        if !t.analyze_path.is_empty() {
            for ext in ["DAT", "EXT", "2EX"] {
                wanted.push(swap_ext(&t.analyze_path, ext));
            }
        }
        if let Some(art) = ex.db.artwork.get(&t.artwork_id) {
            wanted.push(art.clone());
        }
        for path in wanted {
            if ex.file(&path).is_file() {
                continue;
            }
            match c.read_file(&path).await {
                Ok(d) => write(&root, &path, &d)?,
                Err(e) => tracing::debug!(%path, error = %e, "Datei fehlt auf dem Medium"),
            }
        }
        ex.track(id)
            .ok_or_else(|| NfsError::Status(2, format!("Track {id} im Export")))
    }
}

fn swap_ext(path: &str, ext: &str) -> String {
    match path.rfind('.') {
        Some(i) => format!("{}.{ext}", &path[..i]),
        None => format!("{path}.{ext}"),
    }
}

fn write(root: &Path, export_path: &str, data: &[u8]) -> Result<(), NfsError> {
    let mut p = root.to_path_buf();
    for part in export_path
        .split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != "..")
    {
        p.push(part);
    }
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, data)?;
    Ok(())
}
