//! The event stream: parse notifications and apply them to a snapshot.
//! Pure, so it tests without a core.

use crate::types::Terminal;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Notification {
    Event {
        seq: u64,
        kind: String,
        terminal: Option<String>,
        machine: Option<String>,
        data: Value,
    },
    Heartbeat {
        seq: u64,
    },
    /// The subscriber fell behind (or a remote reconnected): a fresh snapshot.
    Missed {
        seq: u64,
        terminals: Vec<Terminal>,
    },
}

impl Notification {
    /// Parse one JSON line. `None` for anything that isn't a notification we know.
    pub fn parse(line: &[u8]) -> Option<Notification> {
        let v: Value = serde_json::from_slice(line).ok()?;
        let p = v.get("params")?;
        let seq = p.get("seq")?.as_u64()?;
        match v.get("method")?.as_str()? {
            "event" => Some(Notification::Event {
                seq,
                kind: p.get("type")?.as_str()?.to_owned(),
                terminal: p.get("terminal").and_then(Value::as_str).map(str::to_owned),
                machine: p.get("machine").and_then(Value::as_str).map(str::to_owned),
                data: p.get("data").cloned().unwrap_or(Value::Null),
            }),
            "heartbeat" => Some(Notification::Heartbeat { seq }),
            "missed" => Some(Notification::Missed {
                seq,
                terminals: serde_json::from_value(p.get("terminals")?.clone()).ok()?,
            }),
            _ => None,
        }
    }
}

/// What applying a notification did, so the UI knows what to redraw or refetch.
#[derive(Clone, Debug, PartialEq)]
pub enum Applied {
    /// Older than the snapshot (after a `missed`): dropped.
    Stale,
    Heartbeat,
    /// A terminal's record changed (or appeared).
    Changed(String),
    Removed(String),
    /// A `restarted` kept terminal: reattach.
    Restarted(String),
    /// Replaced wholesale: redraw everything and re-read documents.
    Resynced,
    /// A document changed; `machine` is set for documents on another machine.
    Doc {
        name: String,
        version: u64,
        machine: Option<String>,
    },
    /// An event type this client doesn't know, or for a terminal it doesn't have.
    Ignored,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub seq: u64,
    pub terminals: BTreeMap<String, Terminal>,
}

impl Snapshot {
    pub fn new(seq: u64, terminals: Vec<Terminal>) -> Snapshot {
        Snapshot {
            seq,
            terminals: terminals.into_iter().map(|t| (t.address.clone(), t)).collect(),
        }
    }

    pub fn apply(&mut self, n: Notification) -> Applied {
        match n {
            Notification::Heartbeat { .. } => Applied::Heartbeat,
            Notification::Missed { seq, terminals } => {
                *self = Snapshot::new(seq, terminals);
                Applied::Resynced
            }
            Notification::Event { seq, .. } if seq <= self.seq => Applied::Stale,
            Notification::Event {
                seq,
                kind,
                terminal,
                machine,
                data,
            } => {
                self.seq = seq;
                if kind == "doc" {
                    let name = data.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
                    let version = data.get("version").and_then(Value::as_u64).unwrap_or(0);
                    return Applied::Doc { name, version, machine };
                }
                let Some(addr) = terminal else { return Applied::Ignored };
                self.apply_terminal(&kind, addr, data)
            }
        }
    }

    fn apply_terminal(&mut self, kind: &str, addr: String, data: Value) -> Applied {
        match kind {
            "created" | "restarted" => {
                let Ok(t) = serde_json::from_value::<Terminal>(data) else {
                    return Applied::Ignored;
                };
                self.terminals.insert(addr.clone(), t);
                return if kind == "restarted" {
                    Applied::Restarted(addr)
                } else {
                    Applied::Changed(addr)
                };
            }
            "removed" => {
                return if self.terminals.remove(&addr).is_some() {
                    Applied::Removed(addr)
                } else {
                    Applied::Ignored
                };
            }
            "labels" => {
                if let Some(Ok(t)) = data
                    .get("record")
                    .map(|r| serde_json::from_value::<Terminal>(r.clone()))
                {
                    self.terminals.insert(addr.clone(), t);
                }
            }
            _ => {}
        }
        let Some(t) = self.terminals.get_mut(&addr) else {
            return Applied::Ignored;
        };
        match kind {
            "lost" | "finished" => {
                t.reason = de(&data, "reason");
                t.exit_code = de(&data, "exit_code");
                t.signal = de(&data, "signal");
                t.finished_ms = de(&data, "finished_ms");
                t.foreground = None;
                if kind == "lost" {
                    t.pid = None;
                } else {
                    t.state = "finished".into();
                }
            }
            "keep" => t.keep = de(&data, "keep").unwrap_or(false),
            "status" => {
                t.status = de(&data, "status");
                t.records = de(&data, "records").unwrap_or_default();
                t.reported = de(&data, "reported").unwrap_or(false);
            }
            "turn" => t.turn = de(&data, "turn").unwrap_or(t.turn),
            "title" => t.title = de(&data, "title"),
            "labels" => t.labels = de(&data, "labels").unwrap_or_default(),
            "size" => {
                t.cols = de(&data, "cols").unwrap_or(t.cols);
                t.rows = de(&data, "rows").unwrap_or(t.rows);
            }
            // The data is the record itself, or null.
            "foreground" => t.foreground = serde_json::from_value(data).ok(),
            _ => return Applied::Ignored,
        }
        Applied::Changed(addr)
    }
}

fn de<T: serde::de::DeserializeOwned>(data: &Value, key: &str) -> Option<T> {
    serde_json::from_value(data.get(key).cloned().unwrap_or(Value::Null)).ok()
}

/// Splits a byte stream into lines; events can be cut anywhere across frames.
#[derive(Default)]
pub(crate) struct Lines(Vec<u8>);

impl Lines {
    pub fn push(&mut self, bytes: &[u8], mut each: impl FnMut(&[u8])) {
        self.0.extend_from_slice(bytes);
        let mut start = 0;
        while let Some(i) = self.0[start..].iter().position(|&b| b == b'\n') {
            let line = &self.0[start..start + i];
            if !line.is_empty() {
                each(line);
            }
            start += i + 1;
        }
        self.0.drain(..start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn term(addr: &str) -> Value {
        json!({"address": addr, "state": "live", "argv": ["sh"], "cwd": "/", "labels": {}, "cols": 80, "rows": 24,
               "pid": 42, "turn": 0, "screen": "x", "created_ms": 1})
    }

    fn ev(seq: u64, kind: &str, terminal: &str, data: Value) -> Notification {
        let line =
            json!({"jsonrpc":"2.0","method":"event","params":{"seq":seq,"type":kind,"terminal":terminal,"data":data}});
        Notification::parse(line.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn events_build_up_a_terminal() {
        let mut s = Snapshot::new(10, vec![]);
        assert_eq!(
            s.apply(ev(11, "created", "t1", term("t1"))),
            Applied::Changed("t1".into())
        );
        s.apply(ev(12, "title", "t1", json!({"title": "vim"})));
        s.apply(ev(
            13,
            "status",
            "t1",
            json!({"status": {"state": "blocked", "kind": "permission"}, "records": {}, "reported": true}),
        ));
        s.apply(ev(14, "labels", "t1", json!({"labels": {"corgi.window": "w1"}})));
        s.apply(ev(15, "size", "t1", json!({"cols": 120, "rows": 40})));
        s.apply(ev(
            16,
            "foreground",
            "t1",
            json!({"pid": 7, "name": "vim", "argv": ["vim"], "cwd": "/src"}),
        ));
        let t = &s.terminals["t1"];
        assert_eq!(t.title.as_deref(), Some("vim"));
        assert_eq!(t.status.as_ref().unwrap().kind.as_deref(), Some("permission"));
        assert_eq!(t.labels["corgi.window"], "w1");
        assert_eq!((t.cols, t.rows), (120, 40));
        assert_eq!(t.foreground.as_ref().unwrap().name, "vim");
        assert_eq!(t.extra["screen"], "x", "unknown fields survive");
    }

    #[test]
    fn lost_keeps_it_live_finished_ends_it() {
        let mut s = Snapshot::new(0, vec![serde_json::from_value(term("t1")).unwrap()]);
        s.apply(ev(
            1,
            "lost",
            "t1",
            json!({"reason": "signaled", "exit_code": null, "signal": 9, "finished_ms": null}),
        ));
        let t = &s.terminals["t1"];
        assert!(t.is_live() && t.pid.is_none());
        assert_eq!(t.reason.as_deref(), Some("signaled"));
        assert_eq!(
            s.apply(ev(2, "restarted", "t1", term("t1"))),
            Applied::Restarted("t1".into())
        );
        s.apply(ev(
            3,
            "finished",
            "t1",
            json!({"reason": "exited", "exit_code": 3, "signal": null, "finished_ms": 5}),
        ));
        let t = &s.terminals["t1"];
        assert_eq!((t.state.as_str(), t.exit_code), ("finished", Some(3)));
        assert_eq!(
            s.apply(ev(4, "removed", "t1", json!({}))),
            Applied::Removed("t1".into())
        );
        assert!(s.terminals.is_empty());
    }

    #[test]
    fn missed_replaces_and_older_events_are_dropped() {
        let mut s = Snapshot::new(0, vec![]);
        let missed = json!({"jsonrpc":"2.0","method":"missed","params":{"seq":50,"terminals":[term("t9")]}});
        assert_eq!(
            s.apply(Notification::parse(missed.to_string().as_bytes()).unwrap()),
            Applied::Resynced
        );
        assert_eq!(s.apply(ev(49, "created", "t1", term("t1"))), Applied::Stale);
        assert_eq!(s.terminals.keys().collect::<Vec<_>>(), ["t9"]);
    }

    #[test]
    fn doc_events_and_unknowns() {
        let mut s = Snapshot::new(0, vec![]);
        let doc = json!({"jsonrpc":"2.0","method":"event","params":{"seq":1,"type":"doc","machine":"vps","data":{"name":"corgi","version":3}}});
        assert_eq!(
            s.apply(Notification::parse(doc.to_string().as_bytes()).unwrap()),
            Applied::Doc {
                name: "corgi".into(),
                version: 3,
                machine: Some("vps".into())
            }
        );
        assert_eq!(s.apply(ev(2, "title", "t404", json!({"title": "x"}))), Applied::Ignored);
        assert_eq!(s.apply(ev(3, "future-thing", "t1", term("t1"))), Applied::Ignored);
    }

    #[test]
    fn lines_survive_any_split() {
        let text = b"{\"a\":1}\n{\"b\":2}\n\n{\"c\":3}\n";
        for cut in 0..text.len() {
            let mut got = Vec::new();
            let mut l = Lines::default();
            l.push(&text[..cut], |x| got.push(x.to_vec()));
            l.push(&text[cut..], |x| got.push(x.to_vec()));
            assert_eq!(
                got,
                vec![b"{\"a\":1}".to_vec(), b"{\"b\":2}".to_vec(), b"{\"c\":3}".to_vec()],
                "cut {cut}"
            );
        }
    }
}
