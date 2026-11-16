use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The core answered with a JSON-RPC error. `kind` is the stable name
    /// (`not_found`, `conflict`, `unavailable`, ...); `hint` says what to do.
    Rpc {
        code: i64,
        message: String,
        kind: Option<String>,
        hint: Option<String>,
    },
    /// The connection is gone (core stopped, restarted, or `hc connect` exited).
    Closed,
    /// `hc` couldn't be started or reached; carries its own message when it gave one.
    Connect(String),
    /// The core can't serve this client (generation, version, methods).
    Mismatch(String),
    /// The core sent something this client doesn't understand.
    Protocol(String),
    Timeout,
    Io(std::io::Error),
}

impl Error {
    /// The RPC error kind, if this is one.
    pub fn kind(&self) -> Option<&str> {
        match self {
            Error::Rpc { kind, .. } => kind.as_deref(),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Rpc { message, hint, .. } => match hint {
                Some(h) => write!(f, "{message} ({h})"),
                None => write!(f, "{message}"),
            },
            Error::Closed => write!(f, "connection to herdr-core closed"),
            Error::Connect(m) => write!(f, "can't reach herdr-core: {m}"),
            Error::Mismatch(m) => write!(f, "{m}"),
            Error::Protocol(m) => write!(f, "protocol error: {m}"),
            Error::Timeout => write!(f, "herdr-core didn't answer in time"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::Protocol(e.to_string())
    }
}
