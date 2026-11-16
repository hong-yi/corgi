//! `corgi doctor`: can corgi reach herdr-core, can that core serve it, and
//! is any corgi running where it shouldn't (inside its own pane)?
//! Exits 1 when it finds a problem.

use hc_proto::{Client, Transport};
use std::path::Path;

pub fn run(session: &str, socket: Option<&Path>) -> anyhow::Result<()> {
    let transport = match socket {
        #[cfg(unix)]
        Some(path) => Transport::Socket(path.to_owned()),
        #[cfg(not(unix))]
        Some(_) => anyhow::bail!("--socket is only supported on Unix; use HC_SESSION"),
        None => Transport::hc(Some(session)),
    };
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(async {
        println!("corgi {}", env!("CARGO_PKG_VERSION"));
        print!("core   ");
        let client = match Client::connect(&transport).await {
            Ok(c) => c,
            Err(e) => {
                println!("✗ {e}");
                std::process::exit(1);
            }
        };
        let h = client.hello();
        println!(
            "✓ herdr-core {} on \"{}\" (session {session}, protocol generation {})",
            h.version, h.core.name, h.generation
        );
        println!("       features: {}", h.features.join(", "));
        let list = client.call("terminal.list", serde_json::json!({})).await?;
        let terminals = list["terminals"].as_array().map_or(0, Vec::len);
        let machines = list["machines"].as_array().map_or(0, Vec::len);
        println!("       {terminals} live terminal(s), {machines} other machine(s)");
        let doc = client.doc_get("corgi", None).await?;
        println!(
            "layout doc \"corgi\": {}",
            if doc.version == 0 {
                "not written yet".to_string()
            } else {
                format!("version {}", doc.version)
            }
        );
        // corgi processes in core terminals. Inside a corgi pane, one shows itself forever.
        let ev = client.subscribe(serde_json::json!({})).await?;
        let me = crate::nesting::own_terminal(&client).await;
        let mut problems = 0;
        let corgis: Vec<_> = ev
            .snapshot
            .terminals
            .values()
            .filter(|t| t.is_live() && crate::nesting::runs_corgi(t))
            .collect();
        if corgis.is_empty() {
            println!("corgi in core terminals: none");
        } else {
            println!("corgi in core terminals:");
        }
        for t in corgis {
            let pid = t.foreground.as_ref().map_or(0, |f| f.pid);
            let here = if me.as_deref() == Some(&t.address) {
                " (this doctor runs here)"
            } else {
                ""
            };
            if crate::nesting::is_corgi_pane(t) {
                problems += 1;
                println!(
                    "  ✗ {} (pid {pid}){here}: runs inside a corgi pane, so it shows itself.\n    \
                     Leave it with C-b C-b d from the corgi showing {0}, or stop it: kill {pid}",
                    t.address
                );
            } else {
                println!("  ✓ {} (pid {pid}){here}", t.address);
            }
        }
        if let Some(me) = &me {
            println!("this doctor runs in core terminal {me}");
        }
        if problems > 0 {
            std::process::exit(1);
        }
        Ok(())
    })
}
