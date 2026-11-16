//! Against a real herdr-core. Each test gets its own core: its own session,
//! state and config under target/tmp/h/<id>. The user's core is never touched.
//!
//! Sockets use core's default path inside that state dir, so names stay short
//! (macOS caps socket paths at 104 bytes). Don't use a relative HC_SOCKET: on
//! hc 0.1.19 the holder isn't adopted across `server.restart` with one, and it
//! outlives `server stop` (see docs/dev/adr/0002-hc-proto.md).
//! Skipped (with a note) when `hc` isn't installed; set HC_BIN to pick one.

use hc_proto::{Applied, Client, Error, StreamFrame, Transport};
use serde_json::json;
use std::ffi::OsString;
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

struct Core {
    dir: PathBuf,
    session: String,
}

fn hc_bin() -> OsString {
    std::env::var_os("HC_BIN").unwrap_or_else(|| "hc".into())
}

impl Core {
    /// `id`: one or two letters, unique per test.
    fn start(id: &str) -> Option<Core> {
        if std::process::Command::new(hc_bin()).arg("--version").output().is_err() {
            eprintln!("skipping: hc not installed");
            return None;
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("h").join(id);
        let session = format!("h{id}");
        // The longest socket core makes: <state>/herdr-core/<s>/<s>.holder.xxxxxxxx.sock
        let longest = dir
            .join("s/herdr-core")
            .join(&session)
            .join(format!("{session}.holder.xxxxxxxx.sock"));
        assert!(
            longest.as_os_str().len() < 104,
            "test dir too deep for a unix socket: {}",
            longest.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Some(Core { dir, session })
    }

    fn env(&self) -> Vec<(OsString, OsString)> {
        vec![
            ("HC_SESSION".into(), self.session.clone().into()),
            ("XDG_STATE_HOME".into(), self.dir.join("s").into()),
            ("XDG_CONFIG_HOME".into(), self.dir.join("c").into()),
            ("XDG_RUNTIME_DIR".into(), self.dir.join("r").into()),
        ]
    }

    fn transport(&self) -> Transport {
        Transport::Spawn {
            program: hc_bin().into(),
            args: vec!["connect".into()],
            cwd: Some(self.dir.clone()),
            env: self.env(),
        }
    }

    async fn connect(&self) -> Client {
        within(20, Client::connect(&self.transport())).await.expect("connect")
    }
}

impl Core {
    fn hc(&self, args: &[&str]) -> Option<serde_json::Value> {
        let out = std::process::Command::new(hc_bin())
            .args(args)
            .current_dir(&self.dir)
            .envs(self.env())
            .output()
            .ok()?;
        serde_json::from_slice(&out.stdout).ok()
    }
}

impl Drop for Core {
    /// Stop the core; then make sure no holder of ours is left running.
    fn drop(&mut self) {
        let holders: Vec<i64> = self
            .hc(&["server", "status"])
            .and_then(|s| s["holders"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|h| h["pid"].as_i64())
            .collect();
        self.hc(&["server", "stop"]);
        for pid in holders {
            let _ = std::process::Command::new("kill").arg(pid.to_string()).output();
        }
    }
}

async fn within<T>(secs: u64, f: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(secs), f)
        .await
        .expect("timed out")
}

/// Read stream output into an emulator until `done` holds for its screen text.
async fn screen_until(a: &mut hc_proto::Attach, vt: &mut corgi_vt::Vt, done: impl Fn(&str) -> bool) -> String {
    within(10, async {
        loop {
            match a.recv().await.expect("stream ended") {
                StreamFrame::Data(b) => vt.write(&b),
                StreamFrame::Close(r) => panic!("closed early: {r:?}"),
                StreamFrame::Resize(..) => {}
            }
            let f = vt.frame().unwrap();
            let text: String = (0..f.rows)
                .map(|y| {
                    (0..f.cols)
                        .map(|x| f.cell(x, y).text.as_str().chars().next().unwrap_or(' '))
                        .collect::<String>()
                        + "\n"
                })
                .collect();
            if done(&text) {
                return text;
            }
        }
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn hello_run_list_and_errors() {
    let Some(core) = Core::start("b") else { return };
    let c = core.connect().await;
    assert_eq!(c.hello().generation, 1);
    assert!(c.hello().has("framed") && c.hello().has("attach") && c.hello().has("events"));

    let t = within(10, c.run(&["printf", "hi\\n"], json!({"wait": true, "timeout": 5})))
        .await
        .unwrap();
    assert!(t.address.starts_with('t'), "{t:?}");
    let list = within(5, c.call("terminal.list", json!({"all": true}))).await.unwrap();
    assert!(
        list["terminals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["address"] == t.address.as_str())
    );

    let err = within(5, c.get("t999")).await.unwrap_err();
    assert_eq!(err.kind(), Some("not_found"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_paints_takes_input_resizes_and_detaches() {
    let Some(core) = Core::start("a") else { return };
    let c = core.connect().await;
    let t = within(10, c.run(&["cat"], json!({"size": {"cols": 40, "rows": 6}})))
        .await
        .unwrap();
    let mut a = within(10, c.attach(&t.address, 40, 6, json!({"history": 100})))
        .await
        .unwrap();
    let mut vt = corgi_vt::Vt::new(40, 6).unwrap();

    // First paint begins with CAN.
    match within(10, a.recv()).await {
        Some(StreamFrame::Data(b)) => {
            assert_eq!(b.first(), Some(&0x18), "first paint starts with CAN: {b:?}");
            vt.write(&b);
        }
        other => panic!("expected first paint, got {other:?}"),
    }

    a.input(b"hello corgi\r");
    // The tty echoes the line, then cat prints it back.
    let screen = screen_until(&mut a, &mut vt, |s| s.matches("hello corgi").count() >= 2).await;
    assert!(screen.lines().next().unwrap().starts_with("hello corgi"), "{screen}");

    a.resize(60, 10);
    within(10, async {
        loop {
            let t = c.get(&t.address).await.unwrap();
            if (t.cols, t.rows) == (60, 10) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    within(5, a.detach()).await;
    assert!(
        within(5, c.get(&t.address)).await.unwrap().is_live(),
        "detach leaves it running"
    );
    within(5, c.call("terminal.kill", json!({"terminal": t.address})))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn exit_closes_the_stream_with_its_reason() {
    let Some(core) = Core::start("x") else { return };
    let c = core.connect().await;
    let t = within(10, c.run(&["sh", "-c", "sleep 0.5; exit 3"], json!({})))
        .await
        .unwrap();
    let mut a = within(10, c.attach(&t.address, 80, 24, json!({}))).await.unwrap();
    let reason = within(10, async {
        loop {
            if let Some(StreamFrame::Close(r)) = a.recv().await {
                return r;
            }
        }
    })
    .await
    .expect("a reason");
    assert_eq!((reason.reason.as_deref(), reason.exit_code), (Some("exited"), Some(3)));
    assert!(reason.is_final());
}

#[tokio::test(flavor = "multi_thread")]
async fn events_follow_terminals_and_documents() {
    let Some(core) = Core::start("e") else { return };
    let c = core.connect().await;
    let mut ev = within(10, c.subscribe(json!({}))).await.unwrap();

    let t = within(
        10,
        c.run(&["sh", "-c", "sleep 0.3"], json!({"labels": {"corgi.window": "w1"}})),
    )
    .await
    .unwrap();
    within(10, async {
        loop {
            ev.next().await.expect("events ended");
            if ev.snapshot.terminals.get(&t.address).is_some_and(|t| !t.is_live()) {
                break;
            }
        }
    })
    .await;
    let rec = &ev.snapshot.terminals[&t.address];
    assert_eq!(rec.labels["corgi.window"], "w1");
    assert_eq!((rec.reason.as_deref(), rec.exit_code), (Some("exited"), Some(0)));

    // Documents: compare-and-swap, with an event per change.
    let doc = within(5, c.doc_get("corgi-test", None)).await.unwrap();
    assert_eq!((doc.value.clone(), doc.version), (serde_json::Value::Null, 0));
    let v1 = within(5, c.doc_set("corgi-test", &json!({"workspaces": []}), 0, None))
        .await
        .unwrap();
    assert!(v1 > 0);
    let stale = within(5, c.doc_set("corgi-test", &json!({"workspaces": [1]}), 0, None))
        .await
        .unwrap_err();
    assert_eq!(stale.kind(), Some("conflict"), "{stale}");
    within(10, async {
        loop {
            if let Some(Applied::Doc { name, version, .. }) = ev.next().await
                && name == "corgi-test"
                && version == v1
            {
                break;
            }
        }
    })
    .await;
    assert_eq!(
        within(5, c.doc_get("corgi-test", None)).await.unwrap().value,
        json!({"workspaces": []})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn terminals_survive_a_core_restart_and_we_reconnect() {
    let Some(core) = Core::start("r") else { return };
    let c = core.connect().await;
    let t = within(10, c.run(&["cat"], json!({"keep": true}))).await.unwrap();
    // The restart answers (or the line drops) and the connection ends.
    let _ = within(20, c.call("server.restart", json!({}))).await;
    within(20, c.closed()).await;
    assert!(matches!(c.call("terminal.list", json!({})).await, Err(Error::Closed)));

    let c2 = within(30, Client::reconnect(&core.transport(), Duration::from_millis(500)))
        .await
        .unwrap();
    let after = within(10, c2.get(&t.address)).await.unwrap();
    assert!(after.is_live(), "{after:?}");
    within(5, c2.call("terminal.kill", json!({"terminal": t.address})))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_hc_says_how_to_install() {
    let t = Transport::Spawn {
        program: "hc-definitely-missing".into(),
        args: vec![],
        cwd: None,
        env: vec![],
    };
    let Err(Error::Connect(msg)) = Client::connect(&t).await else {
        panic!("expected a connect error")
    };
    assert!(msg.contains("install"), "{msg}");
}
