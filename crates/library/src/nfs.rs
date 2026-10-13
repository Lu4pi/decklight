//! NFSv2-Zugriff auf die Medien in den Playern (USB/SD), wie Crate Digger.
//!
//! Der dbserver der CDJs antwortet nur Anfragen von Player-Nummern 1–4; steht
//! nur ein Player im Netz, gibt es niemanden, als den wir uns ausgeben könnten.
//! Die Player geben ihre Medien aber zusätzlich per NFS frei: `/B/` ist der
//! SD-Slot, `/C/` der USB-Slot. Darüber lesen wir `export.pdb` und die
//! ANLZ-Dateien direkt vom Stick. Dateinamen erwarten die Player in UTF-16LE.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use thiserror::Error;
use tokio::net::UdpSocket;

pub const PORTMAP_PORT: u16 = 111;
const PORTMAP: u32 = 100_000;
const MOUNT: u32 = 100_005;
const NFS: u32 = 100_003;
const FH: usize = 32;
const FATTR: usize = 17 * 4;
/// Bytes pro READ: klein genug, damit kein Player die UDP-Antwort verwirft.
pub const READ_SIZE: u32 = 2048;
const TIMEOUT: Duration = Duration::from_millis(800);
const RETRIES: u32 = 4;

#[derive(Debug, Error)]
pub enum NfsError {
    #[error("Netzwerk: {0}")]
    Io(#[from] std::io::Error),
    #[error("Player antwortet nicht per NFS")]
    Timeout,
    #[error("RPC abgelehnt")]
    Rejected,
    #[error("NFS-Fehler {0} bei {1}")]
    Status(u32, String),
    #[error("Antwort zu kurz")]
    Short,
    #[error("Slot {0} hat keine NFS-Freigabe")]
    NoSlot(u8),
}

/// Größe und Änderungszeit einer Datei auf dem Player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileAttr {
    pub size: u32,
    pub mtime: u32,
}

/// Freigabe-Pfad je Slot-Byte aus dem CdjStatus (2 = SD, 3 = USB).
pub fn export_path(slot: u8) -> Option<&'static str> {
    match slot {
        2 => Some("/B/"),
        3 => Some("/C/"),
        _ => None,
    }
}

pub struct NfsClient {
    sock: UdpSocket,
    ip: IpAddr,
    nfs_port: u16,
    root: [u8; FH],
    xid: u32,
}

impl NfsClient {
    /// Medium im Slot `slot` des Players unter `ip` einhängen.
    pub async fn mount(ip: IpAddr, slot: u8) -> Result<Self, NfsError> {
        let path = export_path(slot).ok_or(NfsError::NoSlot(slot))?;
        Self::mount_via(ip, PORTMAP_PORT, path).await
    }

    pub async fn mount_via(ip: IpAddr, portmap: u16, export: &str) -> Result<Self, NfsError> {
        let sock = UdpSocket::bind(match ip {
            IpAddr::V4(_) => "0.0.0.0:0",
            IpAddr::V6(_) => "[::]:0",
        })
        .await?;
        let mut c = Self {
            sock,
            ip,
            nfs_port: 0,
            root: [0; FH],
            xid: std::process::id().wrapping_mul(7919),
        };
        let mount_port = c.getport(portmap, MOUNT, 1).await?;
        c.nfs_port = c.getport(portmap, NFS, 2).await?;
        let mut args = Vec::new();
        put_string(&mut args, export.as_bytes());
        let r = c.call(mount_port, MOUNT, 1, 1, &args).await?;
        let mut rd = Reader(&r);
        let status = rd.u32()?;
        if status != 0 {
            return Err(NfsError::Status(status, export.into()));
        }
        c.root.copy_from_slice(rd.take(FH)?);
        Ok(c)
    }

    async fn getport(&mut self, portmap: u16, prog: u32, vers: u32) -> Result<u16, NfsError> {
        let mut args = Vec::new();
        for v in [prog, vers, 17, 0] {
            put_u32(&mut args, v);
        }
        let r = self.call(portmap, PORTMAP, 2, 3, &args).await?;
        let port = Reader(&r).u32()?;
        u16::try_from(port)
            .ok()
            .filter(|&p| p != 0)
            .ok_or(NfsError::Rejected)
    }

    /// Ein RPC-Aufruf mit AUTH_UNIX; liefert die Ergebnisdaten.
    async fn call(
        &mut self,
        port: u16,
        prog: u32,
        vers: u32,
        proc_: u32,
        args: &[u8],
    ) -> Result<Vec<u8>, NfsError> {
        self.xid = self.xid.wrapping_add(1);
        let xid = self.xid;
        let mut m = Vec::with_capacity(64 + args.len());
        for v in [xid, 0, 2, prog, vers, proc_] {
            put_u32(&mut m, v);
        }
        // AUTH_UNIX: Stempel, leerer Rechnername, uid 0, gid 0, keine Gruppen.
        put_u32(&mut m, 1);
        put_u32(&mut m, 20);
        for v in [0, 0, 0, 0, 0] {
            put_u32(&mut m, v);
        }
        put_u32(&mut m, 0);
        put_u32(&mut m, 0);
        m.extend_from_slice(args);
        let to = SocketAddr::new(self.ip, port);
        let mut buf = vec![0u8; 65_536];
        for _ in 0..RETRIES {
            self.sock.send_to(&m, to).await?;
            let deadline = tokio::time::Instant::now() + TIMEOUT;
            loop {
                let Ok(r) = tokio::time::timeout_at(deadline, self.sock.recv_from(&mut buf)).await
                else {
                    break; // erneut senden
                };
                let (n, _) = r?;
                let mut rd = Reader(&buf[..n]);
                if rd.u32()? != xid || rd.u32()? != 1 {
                    continue; // alte Antwort
                }
                if rd.u32()? != 0 {
                    return Err(NfsError::Rejected);
                }
                rd.u32()?; // verf flavor
                let len = rd.u32()? as usize;
                rd.take(pad(len))?;
                if rd.u32()? != 0 {
                    return Err(NfsError::Rejected);
                }
                return Ok(rd.0.to_vec());
            }
        }
        Err(NfsError::Timeout)
    }

    async fn lookup_one(
        &mut self,
        dir: [u8; FH],
        name: &str,
    ) -> Result<([u8; FH], FileAttr), NfsError> {
        // Pioneer erwartet UTF-16LE; zur Sicherheit danach noch ASCII/UTF-8.
        let utf16: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut last = NfsError::Short;
        for enc in [utf16.as_slice(), name.as_bytes()] {
            let mut args = dir.to_vec();
            put_string(&mut args, enc);
            let r = self.call(self.nfs_port, NFS, 2, 4, &args).await?;
            let mut rd = Reader(&r);
            let status = rd.u32()?;
            if status != 0 {
                last = NfsError::Status(status, name.into());
                continue;
            }
            let mut fh = [0; FH];
            fh.copy_from_slice(rd.take(FH)?);
            return Ok((fh, attr(rd.take(FATTR)?)));
        }
        Err(last)
    }

    async fn lookup(&mut self, path: &str) -> Result<([u8; FH], FileAttr), NfsError> {
        let mut fh = self.root;
        let mut at = FileAttr { size: 0, mtime: 0 };
        for part in path
            .split(['/', '\\'])
            .filter(|s| !s.is_empty() && *s != "..")
        {
            (fh, at) = self.lookup_one(fh, part).await?;
        }
        Ok((fh, at))
    }

    pub async fn stat(&mut self, path: &str) -> Result<FileAttr, NfsError> {
        Ok(self.lookup(path).await?.1)
    }

    /// Ganze Datei lesen.
    pub async fn read_file(&mut self, path: &str) -> Result<Vec<u8>, NfsError> {
        let (fh, at) = self.lookup(path).await?;
        let mut out = Vec::with_capacity(at.size as usize);
        while (out.len() as u32) < at.size {
            let mut args = fh.to_vec();
            put_u32(&mut args, out.len() as u32);
            put_u32(&mut args, READ_SIZE);
            put_u32(&mut args, 0);
            let r = self.call(self.nfs_port, NFS, 2, 6, &args).await?;
            let mut rd = Reader(&r);
            let status = rd.u32()?;
            if status != 0 {
                return Err(NfsError::Status(status, path.into()));
            }
            rd.take(FATTR)?;
            let len = rd.u32()? as usize;
            let data = rd.take(len)?;
            if data.is_empty() {
                break;
            }
            out.extend_from_slice(data);
        }
        Ok(out)
    }
}

fn attr(b: &[u8]) -> FileAttr {
    let w = |i: usize| u32::from_be_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
    FileAttr {
        size: w(5),
        mtime: w(13),
    }
}

fn pad(n: usize) -> usize {
    n.div_ceil(4) * 4
}

fn put_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_be_bytes());
}

fn put_string(b: &mut Vec<u8>, s: &[u8]) {
    put_u32(b, s.len() as u32);
    b.extend_from_slice(s);
    b.resize(b.len() + pad(s.len()) - s.len(), 0);
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], NfsError> {
        if self.0.len() < n {
            return Err(NfsError::Short);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }

    fn u32(&mut self) -> Result<u32, NfsError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Result<&'a [u8], NfsError> {
        let len = self.u32()? as usize;
        let s = self.take(len)?;
        self.take(pad(len) - len)?;
        Ok(s)
    }
}

/// Minimaler Portmap-/Mount-/NFSv2-Server über ein Verzeichnis, wie ihn ein
/// CDJ anbietet (Namen in UTF-16LE). Für Tests und den Simulator.
pub mod server {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    pub struct NfsServer {
        pub portmap_port: u16,
        _task: tokio::task::JoinHandle<()>,
    }

    impl Drop for NfsServer {
        fn drop(&mut self) {
            self._task.abort();
        }
    }

    /// `exports` ordnet Freigaben (z. B. `/C/`) Verzeichnissen zu.
    pub async fn serve(exports: Vec<(String, PathBuf)>) -> std::io::Result<NfsServer> {
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let port = sock.local_addr()?.port();
        let handles: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
        let task = tokio::spawn(async move {
            let mut buf = vec![0u8; 65_536];
            while let Ok((n, from)) = sock.recv_from(&mut buf).await {
                if let Some(reply) = handle(&buf[..n], port, &exports, &handles) {
                    let _ = sock.send_to(&reply, from).await;
                }
            }
        });
        Ok(NfsServer {
            portmap_port: port,
            _task: task,
        })
    }

    fn fh_for(handles: &Mutex<Vec<PathBuf>>, p: PathBuf) -> [u8; FH] {
        let mut h = handles.lock().unwrap_or_else(|e| e.into_inner());
        let idx = h.iter().position(|q| *q == p).unwrap_or_else(|| {
            h.push(p);
            h.len() - 1
        });
        let mut fh = [0; FH];
        fh[..4].copy_from_slice(&(idx as u32).to_be_bytes());
        fh
    }

    fn path_of(handles: &Mutex<Vec<PathBuf>>, fh: &[u8]) -> Option<PathBuf> {
        let idx = u32::from_be_bytes([fh[0], fh[1], fh[2], fh[3]]) as usize;
        handles.lock().ok()?.get(idx).cloned()
    }

    fn fattr(p: &std::path::Path) -> Vec<u8> {
        let m = std::fs::metadata(p).ok();
        let size = m.as_ref().map_or(0, |m| m.len() as u32);
        let mut b = Vec::new();
        for i in 0..17 {
            put_u32(&mut b, if i == 5 { size } else { 0 });
        }
        b
    }

    fn handle(
        msg: &[u8],
        port: u16,
        exports: &[(String, PathBuf)],
        handles: &Mutex<Vec<PathBuf>>,
    ) -> Option<Vec<u8>> {
        let mut rd = Reader(msg);
        let xid = rd.u32().ok()?;
        rd.take(8).ok()?; // CALL, rpcvers
        let (prog, _vers, proc_) = (rd.u32().ok()?, rd.u32().ok()?, rd.u32().ok()?);
        for _ in 0..2 {
            rd.u32().ok()?;
            rd.string().ok()?;
        }
        let mut out = Vec::new();
        for v in [xid, 1, 0, 0, 0, 0] {
            put_u32(&mut out, v);
        }
        match (prog, proc_) {
            (PORTMAP, 3) => put_u32(&mut out, u32::from(port)),
            (MOUNT, 1) => {
                let dir = String::from_utf8_lossy(rd.string().ok()?).into_owned();
                match exports.iter().find(|(e, _)| *e == dir) {
                    Some((_, root)) => {
                        put_u32(&mut out, 0);
                        out.extend_from_slice(&fh_for(handles, root.clone()));
                    }
                    None => put_u32(&mut out, 2),
                }
            }
            (NFS, 4) => {
                let dir = path_of(handles, rd.take(FH).ok()?)?;
                let raw = rd.string().ok()?;
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                // Nur UTF-16LE akzeptieren, wie die Player.
                let name = String::from_utf16(&units)
                    .ok()
                    .filter(|_| raw.len() % 2 == 0);
                let p = name.map(|n| dir.join(n)).filter(|p| p.exists());
                match p {
                    Some(p) => {
                        put_u32(&mut out, 0);
                        out.extend_from_slice(&fh_for(handles, p.clone()));
                        out.extend_from_slice(&fattr(&p));
                    }
                    None => put_u32(&mut out, 2),
                }
            }
            (NFS, 6) => {
                let p = path_of(handles, rd.take(FH).ok()?)?;
                let off = rd.u32().ok()? as usize;
                let count = rd.u32().ok()? as usize;
                let data = std::fs::read(&p).ok()?;
                let chunk = &data[off.min(data.len())..(off + count).min(data.len())];
                put_u32(&mut out, 0);
                out.extend_from_slice(&fattr(&p));
                put_string(&mut out, chunk);
            }
            _ => return None,
        }
        Some(out)
    }
}
