//! A client for herdr-core (`hc`). See herdr-core's `schema/README.md` for the
//! protocol; this crate follows it and corgi's docs/dev/protocol.md.
//!
//! - [`Client::connect`]: spawn `hc connect` (or open the socket), `hello`,
//!   switch to framed mode, check the core can serve us.
//! - [`Client::call`]: JSON-RPC on channel 0, concurrent.
//! - [`Client::attach`]: a terminal's byte stream with credit handled.
//! - [`Client::subscribe`]: the event stream; [`Snapshot`] applies it.

mod client;
mod error;
mod events;
pub mod types;
pub mod wire;

pub use client::{Attach, AttachTx, Client, Events, StreamFrame, Transport};
pub use error::Error;
pub use events::{Applied, Notification, Snapshot};
pub use types::*;

/// Protocol generation this client speaks.
pub const GENERATION: u64 = 1;
/// Oldest core corgi is tested against.
pub const MIN_CORE: &str = "0.1.19";
/// Methods corgi calls; a core missing any of them can't serve it.
pub const USES: &[&str] = &[
    "hello",
    "terminal.run",
    "terminal.list",
    "terminal.get",
    "terminal.attach",
    "terminal.kill",
    "terminal.label",
    "events.subscribe",
    "doc.get",
    "doc.set",
];

/// Whether a core's hello says it can serve corgi.
pub fn check(hello: &HelloResult) -> Result<(), Error> {
    if hello.generation != GENERATION {
        return Err(Error::Mismatch(format!(
            "core speaks protocol generation {}, corgi speaks {GENERATION}",
            hello.generation
        )));
    }
    if semver(&hello.version) < semver(MIN_CORE) {
        return Err(Error::Mismatch(format!(
            "core {} is too old; corgi needs {MIN_CORE} or newer (run `hc update`)",
            hello.version
        )));
    }
    let missing: Vec<&str> = USES
        .iter()
        .copied()
        .filter(|m| !hello.methods.iter().any(|h| h == m))
        .collect();
    if !missing.is_empty() {
        return Err(Error::Mismatch(format!(
            "core {} lacks {}",
            hello.version,
            missing.join(", ")
        )));
    }
    Ok(())
}

fn semver(v: &str) -> (u64, u64, u64) {
    let mut it = v.split(['.', '-', '+']).map(|p| p.parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(generation: u64, version: &str, methods: &[&str]) -> HelloResult {
        HelloResult {
            generation,
            version: version.into(),
            methods: methods.iter().map(|m| m.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn hello_gates_on_generation_version_and_methods() {
        assert!(check(&hello(1, "0.1.19", USES)).is_ok());
        assert!(check(&hello(1, "0.2.0-rc1", USES)).is_ok());
        assert!(matches!(check(&hello(2, "9.9.9", USES)), Err(Error::Mismatch(m)) if m.contains("generation 2")));
        assert!(matches!(check(&hello(1, "0.1.11", USES)), Err(Error::Mismatch(m)) if m.contains("too old")));
        assert!(matches!(check(&hello(1, "0.1.19", &USES[1..])), Err(Error::Mismatch(m)) if m.contains("lacks hello")));
    }
}
