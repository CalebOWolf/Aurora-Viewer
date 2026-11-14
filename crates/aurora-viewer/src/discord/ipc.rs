//! Discord's local RPC connection: the socket (a named pipe on Windows,
//! a Unix socket elsewhere) the Discord client opens for the programs of
//! the same user, and its frames.
//!
//! The Legacy Viewer goes through the discord-rpc library (Discord_Initialize,
//! Discord_UpdatePresence; fsdiscordconnect.cpp). Its protocol is small and
//! stable, so it is written here instead: a frame is an opcode and a length
//! (two little-endian `u32`) followed by that many bytes of JSON. The first
//! frame is the handshake (`{"v":1,"client_id":…}`), Discord answers with a
//! READY dispatch naming the user, and each SET_ACTIVITY command gets an
//! answer carrying its nonce. Nothing leaves the computer from here: Discord
//! itself publishes the activity.

use serde_json::Value;
use std::io::{self, Read, Write};
use std::path::PathBuf;

/// The opcodes of a frame (discord-rpc's `Opcode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl Op {
    fn from_u32(v: u32) -> Option<Op> {
        Some(match v {
            0 => Op::Handshake,
            1 => Op::Frame,
            2 => Op::Close,
            3 => Op::Ping,
            4 => Op::Pong,
            _ => return None,
        })
    }
}

/// A frame larger than this is not Discord's (discord-rpc's MaxRpcFrameSize
/// is 64 KiB): the connection is dropped instead of allocating it.
pub const MAX_FRAME: usize = 64 * 1024;

/// The bytes of one frame.
pub fn encode(op: Op, payload: &Value) -> Vec<u8> {
    let body = payload.to_string();
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&(op as u32).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body.as_bytes());
    out
}

/// The opcode and the length of a frame from its 8-byte header.
pub fn header(bytes: [u8; 8]) -> io::Result<(Op, usize)> {
    let op = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let op = Op::from_u32(op).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("unknown opcode {op}")))?;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("frame of {len} bytes")));
    }
    Ok((op, len))
}

/// The places a Discord client listens, in the order discord-rpc tries them:
/// `discord-ipc-0` to `-9`, under the temporary folders on Unix (with the
/// Flatpak and Snap packages' own folders), as named pipes on Windows.
pub fn candidates() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        (0..10).map(|i| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{i}"))).collect()
    }
    #[cfg(not(windows))]
    {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let roots = unix_roots(var("XDG_RUNTIME_DIR"), var("TMPDIR"), var("TMP"), var("TEMP"));
        let mut out = Vec::new();
        for root in &roots {
            for sub in UNIX_SUBFOLDERS {
                for i in 0..10 {
                    out.push(root.join(sub).join(format!("discord-ipc-{i}")));
                }
            }
        }
        out
    }
}

/// The folders under a temporary folder where a packaged Discord puts its
/// socket: the plain client first, then Flatpak (Discord, Vesktop) and Snap.
#[cfg(not(windows))]
const UNIX_SUBFOLDERS: &[&str] = &[
    "",
    "app/com.discordapp.Discord",
    "app/com.discordapp.DiscordCanary",
    ".flatpak/dev.vencord.Vesktop/xdg-run",
    "snap.discord",
    "snap.discord-canary",
];

/// discord-rpc's GetTempPath: XDG_RUNTIME_DIR, TMPDIR, TMP, TEMP, then /tmp;
/// every one that is set is tried, without repeats.
#[cfg(not(windows))]
fn unix_roots(xdg: Option<PathBuf>, tmpdir: Option<PathBuf>, tmp: Option<PathBuf>, temp: Option<PathBuf>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for root in [xdg, tmpdir, tmp, temp, Some(PathBuf::from("/tmp"))].into_iter().flatten() {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

/// The open connection to a Discord client.
pub struct Conn {
    #[cfg(windows)]
    pipe: std::fs::File,
    #[cfg(not(windows))]
    pipe: std::os::unix::net::UnixStream,
}

impl Conn {
    /// The first Discord socket that accepts. None when no client runs.
    pub fn open() -> Option<Conn> {
        candidates().into_iter().find_map(|path| Self::open_at(&path).ok())
    }

    #[cfg(windows)]
    fn open_at(path: &std::path::Path) -> io::Result<Conn> {
        // a named pipe opens like a file; one handle reads and writes
        let pipe = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Conn { pipe })
    }

    #[cfg(not(windows))]
    fn open_at(path: &std::path::Path) -> io::Result<Conn> {
        let pipe = std::os::unix::net::UnixStream::connect(path)?;
        // a client that stops answering never holds the worker for long
        pipe.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
        pipe.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
        Ok(Conn { pipe })
    }

    /// A connection over a socket the tests made.
    #[cfg(all(test, not(windows)))]
    pub fn from_stream(pipe: std::os::unix::net::UnixStream) -> Conn {
        Conn { pipe }
    }

    pub fn send(&mut self, op: Op, payload: &Value) -> io::Result<()> {
        self.pipe.write_all(&encode(op, payload))?;
        self.pipe.flush()
    }

    /// The next frame. A ping is answered here and skipped; a close frame
    /// ends the connection with its message.
    pub fn read(&mut self) -> io::Result<Value> {
        loop {
            let mut head = [0u8; 8];
            self.pipe.read_exact(&mut head)?;
            let (op, len) = header(head)?;
            let mut body = vec![0u8; len];
            self.pipe.read_exact(&mut body)?;
            let value: Value = serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            match op {
                Op::Frame => return Ok(value),
                Op::Ping => self.send(Op::Pong, &value)?,
                Op::Pong | Op::Handshake => {}
                Op::Close => {
                    let message = value.get("message").and_then(Value::as_str).unwrap_or("closed").to_owned();
                    return Err(io::Error::new(io::ErrorKind::ConnectionAborted, message));
                }
            }
        }
    }

    /// Tells Discord the connection ends (it clears the activity at once).
    pub fn close(mut self) {
        let _ = self.send(Op::Close, &serde_json::json!({}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_frame_is_its_opcode_its_length_and_its_json() {
        let bytes = encode(Op::Handshake, &json!({"v": 1}));
        assert_eq!(&bytes[..4], &0u32.to_le_bytes());
        assert_eq!(&bytes[4..8], &7u32.to_le_bytes());
        assert_eq!(&bytes[8..], br#"{"v":1}"#);
        let head: [u8; 8] = bytes[..8].try_into().expect("8 bytes");
        assert_eq!(header(head).expect("valid"), (Op::Handshake, 7));
    }

    #[test]
    fn a_foreign_or_huge_frame_is_refused() {
        let mut head = [0u8; 8];
        head[..4].copy_from_slice(&9u32.to_le_bytes());
        assert!(header(head).is_err());
        head[..4].copy_from_slice(&1u32.to_le_bytes());
        head[4..].copy_from_slice(&((MAX_FRAME + 1) as u32).to_le_bytes());
        assert!(header(head).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn the_temporary_folders_are_tried_once_each_then_tmp() {
        let roots = unix_roots(
            Some(PathBuf::from("/run/user/1000")),
            Some(PathBuf::from("/tmp")),
            None,
            Some(PathBuf::from("/run/user/1000")),
        );
        assert_eq!(roots, vec![PathBuf::from("/run/user/1000"), PathBuf::from("/tmp")]);
    }

    #[cfg(not(windows))]
    #[test]
    fn a_frame_goes_both_ways_and_a_ping_is_answered() {
        use std::os::unix::net::UnixStream;
        let (a, mut b) = UnixStream::pair().expect("socket pair");
        let mut conn = Conn { pipe: a };
        b.write_all(&encode(Op::Ping, &json!({"n": 1}))).expect("ping");
        b.write_all(&encode(Op::Frame, &json!({"evt": "READY"}))).expect("frame");
        assert_eq!(conn.read().expect("frame")["evt"], "READY");
        let mut head = [0u8; 8];
        b.read_exact(&mut head).expect("pong header");
        let (op, len) = header(head).expect("pong");
        assert_eq!(op, Op::Pong);
        let mut body = vec![0u8; len];
        b.read_exact(&mut body).expect("pong body");
        assert_eq!(body, br#"{"n":1}"#);
    }
}
