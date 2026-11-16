//! The API records corgi reads. Fields corgi doesn't use yet are kept in
//! `extra`, so nothing the core sends is lost when a record is re-serialized.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct CoreInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct HelloResult {
    pub generation: u64,
    pub version: String,
    #[serde(default)]
    pub core: CoreInfo,
    #[serde(default)]
    pub framed: bool,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
}

impl HelloResult {
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// What a program reported through OSC 7501, stored as reported.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Status {
    /// `idle`, `working`, `done`, `blocked`, `error`, or anything else a program says.
    pub state: String,
    /// For `blocked`: `permission`, `question`, `auth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    /// Which program, e.g. `claude-code`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Foreground {
    pub pid: i64,
    pub name: String,
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: String,
}

/// A terminal as the core describes it. `address` is `t7`, or `vps/t7` on another machine.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Terminal {
    pub address: String,
    #[serde(default)]
    pub name: Option<String>,
    /// `live` or `finished`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub rows: u16,
    #[serde(default)]
    pub pid: Option<i64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub signal: Option<i64>,
    #[serde(default)]
    pub finished_ms: Option<u64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default)]
    pub records: Map<String, Value>,
    #[serde(default)]
    pub reported: bool,
    #[serde(default)]
    pub turn: u64,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub keep: bool,
    #[serde(default)]
    pub foreground: Option<Foreground>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Terminal {
    pub fn is_live(&self) -> bool {
        self.state == "live"
    }
}

/// Facts about how the viewer draws, so the core can answer colour and cell-size queries.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Display {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Cell size in pixels, `[w, h]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<[u16; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AttachResult {
    pub channel: u32,
    pub terminal: Terminal,
    #[serde(default)]
    pub automatic_input: bool,
    #[serde(default)]
    pub resize_notifications: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SubscribeResult {
    pub seq: u64,
    #[serde(default)]
    pub terminals: Vec<Terminal>,
    pub channel: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Doc {
    pub value: Value,
    pub version: u64,
}

/// Why a stream ended, from the core's CLOSE payload.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CloseReason {
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub signal: Option<i64>,
}

impl CloseReason {
    /// The program is gone for good: don't reattach.
    pub fn is_final(&self) -> bool {
        matches!(self.reason.as_deref(), Some("exited" | "killed" | "finished"))
    }
}
