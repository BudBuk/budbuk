//! Error types for the connector framework.
//!
//! In Rust, functions that can fail return `Result<T, E>` — either `Ok(value)`
//! or `Err(error)`. We define ONE error type for all connector operations so
//! callers handle failures uniformly.

use thiserror::Error;

/// Everything that can go wrong while a connector does its job.
///
/// `#[derive(Error)]` (from the `thiserror` crate) auto-writes the boilerplate
/// that makes this a proper error type. Each `#[error("...")]` is the message
/// shown when the error is printed. `{0}` inserts the variant's inner value.
#[derive(Debug, Error)]
pub enum ConnectorError {
    /// A network / HTTP call failed (timeout, DNS, connection refused, …).
    #[error("network request failed: {0}")]
    Network(String),

    /// The response came back but we couldn't parse it (bad/unexpected JSON).
    #[error("failed to parse response: {0}")]
    Parse(String),

    /// Credentials were missing, wrong, or rejected.
    #[error("authentication failed: {0}")]
    Auth(String),

    /// The caller asked for a table this connector doesn't expose.
    #[error("unknown table: {0}")]
    UnknownTable(String),

    /// Anything else, with a human-readable message.
    #[error("{0}")]
    Other(String),
}

/// A convenience alias. Instead of writing `Result<T, ConnectorError>`
/// everywhere, functions in this crate just write `Result<T>`.
pub type Result<T> = std::result::Result<T, ConnectorError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_render_for_every_variant() {
        assert_eq!(
            ConnectorError::Network("timeout".into()).to_string(),
            "network request failed: timeout"
        );
        assert_eq!(
            ConnectorError::Parse("bad json".into()).to_string(),
            "failed to parse response: bad json"
        );
        assert_eq!(
            ConnectorError::Auth("401".into()).to_string(),
            "authentication failed: 401"
        );
        assert_eq!(
            ConnectorError::UnknownTable("widgets".into()).to_string(),
            "unknown table: widgets"
        );
        assert_eq!(ConnectorError::Other("boom".into()).to_string(), "boom");
    }

    #[test]
    fn debug_is_available() {
        // Exercises the derived Debug impl.
        let msg = format!("{:?}", ConnectorError::Other("x".into()));
        assert!(msg.contains("Other"));
    }

    #[test]
    fn result_alias_works() {
        fn ok_value() -> Result<i32> {
            Ok(1)
        }
        fn err_value() -> Result<i32> {
            Err(ConnectorError::Other("x".into()))
        }
        assert_eq!(ok_value().unwrap(), 1);
        assert!(err_value().is_err());
    }
}
