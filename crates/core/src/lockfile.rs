//! Parses the Riot Client lockfile.
//!
//! Format (see `docs Common Components > Lockfile Data`, techchrism's
//! valorant-api-docs): `name:pid:port:password:protocol`, a single line of
//! colon-separated fields at
//! `%LOCALAPPDATA%\Riot Games\Riot Client\Config\lockfile`.
//!
//! The password itself may contain `:`, so we take `name`/`pid`/`port` from
//! the front of the line and `protocol` from the back; whatever remains in
//! the middle (rejoined with `:`) is the password.

use std::fmt;

/// The pieces of the lockfile this app actually needs to talk to the local
/// Riot client API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lockfile {
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

/// Why a lockfile string failed to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Fewer than 5 `:`-separated fields (`name:pid:port:password:protocol`).
    TooFewFields,
    /// The `port` field was not a valid `u16`.
    InvalidPort,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::TooFewFields => {
                write!(f, "lockfile needs at least 5 ':'-separated fields")
            }
            ParseError::InvalidPort => write!(f, "lockfile port is not a valid number"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses a raw lockfile string into its [`Lockfile`] pieces.
pub fn parse_lockfile(s: &str) -> Result<Lockfile, ParseError> {
    let trimmed = s.trim_end_matches(['\r', '\n', ' ', '\t']);
    let parts: Vec<&str> = trimmed.split(':').collect();

    // name:pid:port:password:protocol — at least 5 fields, with the
    // password (fields 4..n-1) allowed to contain further ':'.
    if parts.len() < 5 {
        return Err(ParseError::TooFewFields);
    }

    let port_str = parts[2];
    let protocol = parts[parts.len() - 1];
    let password = parts[3..parts.len() - 1].join(":");

    let port: u16 = port_str.parse().map_err(|_| ParseError::InvalidPort)?;

    Ok(Lockfile {
        port,
        password,
        protocol: protocol.to_string(),
    })
}
