//! Discord Rich Presence: the "Playing Aurora Viewer" card of the user's
//! Discord profile, with the region and, if the user shares it, their name.
//!
//! The role of FSDiscordConnect (indra/newview/fsdiscordconnect.cpp,
//! originally LGPL 2.1) and its settings (FSEnableDiscordIntegration,
//! FSShareNameToDiscord, FSMaxSharedMaturity, FSBlacklistedRegionNames, all
//! off or the strictest by default). The Legacy Viewer links the discord-rpc
//! library; here a worker thread speaks Discord's local socket protocol
//! itself (`ipc.rs`) and the rules of what is shown are pure functions
//! (`activity.rs`). Nothing is sent anywhere but to the Discord client
//! running on this computer, and only while the setting is on and the user
//! is in world; the setting off closes the connection, which clears the
//! card at once.
//!
//! A Discord application id names the card ("Playing <application name>")
//! and holds its images. It is not a secret: every Discord program ships its
//! own. The published build takes it from `AURORA_DISCORD_APP_ID` at build
//! time (a repository variable of the Release workflow); Preferences ›
//! Privacy › Discord can name another (`docs/DISCORD.md`).
//!
//! Deliberate differences: the settings are the viewer's, not per account;
//! the connection is made and kept by the switch alone, with no Connect
//! button (the worker retries every 15 seconds while Discord is closed);
//! no marker file between two viewers (Discord shows the last one that
//! wrote).

pub mod activity;
pub mod ipc;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The highest rating of the regions whose name is shared
/// (FSMaxSharedMaturity: 42, 21, 13 or 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Shared {
    /// "Don't share".
    None,
    /// General only, the Legacy Viewer's default (13).
    #[default]
    General,
    /// General and Moderate.
    Moderate,
    /// General, Moderate and Adult.
    Adult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    /// Show the activity on Discord (FSEnableDiscordIntegration, off).
    pub on: bool,
    /// Our name on the card (FSShareNameToDiscord, off).
    pub share_name: bool,
    pub max_maturity: Shared,
    /// Region names never shared (FSBlacklistedRegionNames), ignoring case.
    pub hidden_regions: Vec<String>,
    /// Another Discord application than the built-in one; empty: the
    /// built-in one.
    pub app_id: String,
}

impl Settings {
    /// At most this many regions are kept in the list, each name at most
    /// 64 characters (a region name has 35 at most).
    pub const MAX_HIDDEN: usize = 200;

    pub fn sanitized(mut self) -> Self {
        self.hidden_regions = self
            .hidden_regions
            .into_iter()
            .map(|n| n.trim().chars().take(64).collect::<String>())
            .filter(|n| !n.is_empty())
            .take(Self::MAX_HIDDEN)
            .collect();
        self.app_id = self.app_id.trim().to_owned();
        self
    }

    /// Adds a region to the list, unless it is empty or already there
    /// (FSFloaterDiscord::onAdd compares lower case). True when added.
    pub fn hide_region(&mut self, name: &str) -> bool {
        let name: String = name.trim().chars().take(64).collect();
        if name.is_empty() || self.hidden_regions.len() >= Self::MAX_HIDDEN {
            return false;
        }
        let lower = name.to_lowercase();
        if self.hidden_regions.iter().any(|h| h.to_lowercase() == lower) {
            return false;
        }
        self.hidden_regions.push(name);
        true
    }
}

/// The application id built into this viewer, if its build named one.
pub fn built_in_app_id() -> Option<&'static str> {
    option_env!("AURORA_DISCORD_APP_ID").map(str::trim).filter(|id| valid_app_id(id))
}

/// A Discord application id: a snowflake, 17 to 20 digits.
pub fn valid_app_id(id: &str) -> bool {
    (17..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

/// The application id in use: the user's, else the built-in one.
pub fn app_id(s: &Settings) -> Option<String> {
    let own = s.app_id.trim();
    if !own.is_empty() {
        return valid_app_id(own).then(|| own.to_owned());
    }
    built_in_app_id().map(str::to_owned)
}

/// Where the connection stands, for Preferences.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    /// The setting is off, or the user is not in world.
    #[default]
    Off,
    /// No application id (none built in, or the one typed is not one).
    NoAppId,
    /// No Discord client answers; tried again every 15 seconds.
    NotRunning,
    Connecting,
    /// Connected; the Discord user's name, as Discord gave it.
    Connected(String),
    /// Discord refused the application or the activity (its message).
    Refused(String),
}

/// What the worker should show: None closes the connection.
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub app_id: String,
    pub activity: Value,
}

enum Msg {
    Show(Option<Wanted>),
    Quit,
}

/// The handle the app keeps; the connection lives on its worker thread.
pub struct Presence {
    tx: Option<Sender<Msg>>,
    status: Arc<Mutex<Status>>,
    last: Option<Wanted>,
    /// The app gathers the facts at most once a second.
    pub next_check: Option<Instant>,
}

impl Default for Presence {
    fn default() -> Self {
        Presence {
            tx: None,
            status: Arc::new(Mutex::new(Status::Off)),
            last: None,
            next_check: None,
        }
    }
}

impl Presence {
    pub fn status(&self) -> Status {
        self.status.lock().clone()
    }

    /// Asks for this activity (None: nothing on Discord). Only a change
    /// reaches the worker; the worker starts at the first activity.
    pub fn show(&mut self, wanted: Option<Wanted>) {
        if wanted == self.last {
            return;
        }
        if wanted.is_some() && self.tx.is_none() {
            let (tx, rx) = crossbeam_channel::unbounded();
            let status = self.status.clone();
            match std::thread::Builder::new()
                .name("discord".into())
                .spawn(move || Worker::new(status).run(&rx))
            {
                Ok(_) => self.tx = Some(tx),
                Err(e) => {
                    log::warn!("discord: worker not started ({e})");
                    return;
                }
            }
        }
        if wanted.is_none() {
            // said here, not by the worker: `set_no_app_id` follows
            *self.status.lock() = Status::Off;
        }
        self.last = wanted.clone();
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Show(wanted));
        }
    }

    /// The app has no application id to give: said in Preferences.
    pub fn set_no_app_id(&mut self) {
        self.show(None);
        *self.status.lock() = Status::NoAppId;
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        // the worker closes the connection (the card goes at once); not
        // waited for: a stuck client never holds the exit
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Msg::Quit);
        }
    }
}

/// Between two tries while Discord is closed (discord-rpc reconnects with a
/// back-off of 0.5 to 60 s; Discord's own SDK samples say 15).
const RETRY: Duration = Duration::from_secs(15);

/// Between two activities sent: Discord accepts 5 updates in 20 seconds,
/// and a walk changes the position every second.
const MIN_INTERVAL: Duration = Duration::from_secs(5);

struct Worker {
    status: Arc<Mutex<Status>>,
    conn: Option<(String, ipc::Conn)>,
    wanted: Option<Wanted>,
    /// The activity Discord has.
    sent: Option<Value>,
    next_try: Instant,
    next_send: Instant,
    nonce: u64,
}

impl Worker {
    fn new(status: Arc<Mutex<Status>>) -> Self {
        let now = Instant::now();
        Worker {
            status,
            conn: None,
            wanted: None,
            sent: None,
            next_try: now,
            next_send: now,
            nonce: 0,
        }
    }

    fn set(&self, status: Status) {
        *self.status.lock() = status;
    }

    fn run(mut self, rx: &Receiver<Msg>) {
        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(Msg::Show(w)) => self.wanted = w,
                Ok(Msg::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            // only the latest request matters
            let mut quit = false;
            for msg in rx.try_iter() {
                match msg {
                    Msg::Show(w) => self.wanted = w,
                    Msg::Quit => quit = true,
                }
            }
            if quit {
                break;
            }
            self.step();
        }
        self.disconnect();
    }

    fn disconnect(&mut self) {
        if let Some((_, conn)) = self.conn.take() {
            conn.close();
            log::info!("discord: disconnected");
        }
        self.sent = None;
    }

    fn step(&mut self) {
        let Some(wanted) = self.wanted.clone() else {
            self.disconnect();
            return;
        };
        if self.conn.as_ref().is_some_and(|(id, _)| *id != wanted.app_id) {
            self.disconnect();
            self.next_try = Instant::now();
        }
        let now = Instant::now();
        if self.conn.is_none() {
            if now < self.next_try {
                return;
            }
            self.next_try = now + RETRY;
            self.connect(&wanted.app_id);
        }
        if self.conn.is_none() || self.sent.as_ref() == Some(&wanted.activity) || now < self.next_send {
            return;
        }
        self.next_send = now + MIN_INTERVAL;
        match self.send_activity(&wanted.activity) {
            Ok(()) => self.sent = Some(wanted.activity),
            Err(e) => {
                log::info!("discord: activity not sent ({e})");
                self.disconnect();
                self.set(Status::NotRunning);
            }
        }
    }

    fn connect(&mut self, app_id: &str) {
        let Some(mut conn) = ipc::Conn::open() else {
            self.set(Status::NotRunning);
            return;
        };
        self.set(Status::Connecting);
        let ready = conn
            .send(ipc::Op::Handshake, &json!({ "v": 1, "client_id": app_id }))
            .and_then(|()| conn.read());
        match ready {
            Ok(frame) if frame["evt"] == "READY" => {
                let user = &frame["data"]["user"];
                let name = user["global_name"]
                    .as_str()
                    .filter(|n| !n.is_empty())
                    .or_else(|| user["username"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                log::info!("discord: connected");
                self.set(Status::Connected(name));
                self.conn = Some((app_id.to_owned(), conn));
                self.sent = None;
            }
            Ok(frame) => {
                let message = error_message(&frame);
                log::warn!("discord: handshake refused ({message})");
                self.set(Status::Refused(message));
            }
            // an unknown application id closes the socket with a message
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionAborted => {
                log::warn!("discord: handshake refused ({e})");
                self.set(Status::Refused(e.to_string()));
            }
            Err(e) => {
                log::info!("discord: no answer to the handshake ({e})");
                self.set(Status::NotRunning);
            }
        }
    }

    fn send_activity(&mut self, activity: &Value) -> std::io::Result<()> {
        let Some((_, conn)) = self.conn.as_mut() else {
            return Ok(());
        };
        self.nonce += 1;
        let nonce = self.nonce.to_string();
        let command = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": nonce,
        });
        conn.send(ipc::Op::Frame, &command)?;
        // the answer carries our nonce; an event before it is skipped
        for _ in 0..8 {
            let frame = conn.read()?;
            if frame["nonce"] != nonce.as_str() {
                continue;
            }
            if frame["evt"] == "ERROR" {
                let message = error_message(&frame);
                log::warn!("discord: activity refused ({message})");
                // kept as sent: the same activity is not sent again
                self.set(Status::Refused(message));
            }
            return Ok(());
        }
        Ok(())
    }
}

/// Discord's message of an ERROR frame.
fn error_message(frame: &Value) -> String {
    frame["data"]["message"]
        .as_str()
        .unwrap_or("unknown error")
        .chars()
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_wanted_starts_no_worker_and_keeps_the_no_app_status() {
        let mut p = Presence::default();
        p.show(None);
        assert!(p.tx.is_none());
        p.set_no_app_id();
        assert_eq!(p.status(), Status::NoAppId);
        // the same request again changes nothing
        p.set_no_app_id();
        assert_eq!(p.status(), Status::NoAppId);
        assert!(p.last.is_none());
    }

    #[test]
    fn an_application_id_is_a_snowflake() {
        assert!(valid_app_id("1394782217405862001"));
        assert!(!valid_app_id("12345"));
        assert!(!valid_app_id("13947822174058620x1"));
        assert!(!valid_app_id(""));
    }

    #[test]
    fn the_typed_id_wins_and_a_wrong_one_gives_none() {
        let s = Settings {
            app_id: " 1394782217405862001 ".into(),
            ..Settings::default()
        };
        assert_eq!(app_id(&s).as_deref(), Some("1394782217405862001"));
        let s = Settings {
            app_id: "not an id".into(),
            ..Settings::default()
        };
        assert_eq!(app_id(&s), None);
    }

    #[test]
    fn a_region_is_listed_once_whatever_its_case() {
        let mut s = Settings::default();
        assert!(s.hide_region(" Aurora Plaza "));
        assert!(!s.hide_region("aurora plaza"));
        assert!(!s.hide_region("   "));
        assert_eq!(s.hidden_regions, vec!["Aurora Plaza".to_owned()]);
    }

    #[test]
    fn the_defaults_share_nothing_until_turned_on() {
        let s = Settings::default();
        assert!(!s.on);
        assert!(!s.share_name);
        assert_eq!(s.max_maturity, Shared::General);
        let s = Settings {
            hidden_regions: vec!["  ".into(), " A ".into()],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.hidden_regions, vec!["A".to_owned()]);
    }

    #[cfg(not(windows))]
    #[test]
    fn the_worker_says_handshake_ready_then_sends_the_activity() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        // a fake Discord client at the other end of a socket pair
        let (ours, mut s) = UnixStream::pair().expect("socket pair");
        let server = std::thread::spawn(move || {
            let read = |s: &mut std::os::unix::net::UnixStream| {
                let mut head = [0u8; 8];
                s.read_exact(&mut head).expect("header");
                let (op, len) = ipc::header(head).expect("frame");
                let mut body = vec![0u8; len];
                s.read_exact(&mut body).expect("body");
                (op, serde_json::from_slice::<Value>(&body).expect("json"))
            };
            let (op, hello) = read(&mut s);
            assert_eq!(op, ipc::Op::Handshake);
            assert_eq!(hello["client_id"], "1394782217405862001");
            let ready = json!({"evt": "READY", "data": {"user": {"username": "jane", "global_name": "Jane"}}});
            s.write_all(&ipc::encode(ipc::Op::Frame, &ready)).expect("ready");
            let (_, cmd) = read(&mut s);
            assert_eq!(cmd["cmd"], "SET_ACTIVITY");
            assert_eq!(cmd["args"]["activity"]["state"], "Aurora Plaza (1, 2, 3)");
            let answer = json!({"cmd": "SET_ACTIVITY", "nonce": cmd["nonce"], "data": {}});
            s.write_all(&ipc::encode(ipc::Op::Frame, &answer)).expect("answer");
        });
        let status = Arc::new(Mutex::new(Status::Off));
        let mut w = Worker::new(status.clone());
        let mut conn = ipc::Conn::from_stream(ours);
        conn.send(ipc::Op::Handshake, &json!({"v": 1, "client_id": "1394782217405862001"}))
            .expect("handshake");
        assert_eq!(conn.read().expect("ready")["data"]["user"]["global_name"], "Jane");
        w.conn = Some(("1394782217405862001".into(), conn));
        w.send_activity(&json!({"state": "Aurora Plaza (1, 2, 3)"})).expect("sent");
        server.join().expect("server");
        assert_eq!(*status.lock(), Status::Off);
    }
}
