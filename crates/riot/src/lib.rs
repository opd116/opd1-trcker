//! `opd1-riot`: throttled HTTP access to the local Riot client and to
//! valorant-api.com.
//!
//! - [`throttle::Throttle`]: the token-bucket rate limiter shared by every
//!   remote (pd/glz) call.
//! - [`session::RiotSession`]: lockfile -> local auth -> entitlements ->
//!   remote headers, with auto re-auth on 401/403 and a `ClientGone`
//!   signal the moment the Riot client disappears.
//! - [`endpoints`]: typed wrappers, one per Riot endpoint this app uses,
//!   the response structs they return, and [`endpoints::VisiblePuuid`] —
//!   the type-level guarantee that a hidden player's name is never
//!   requested.

pub mod assets;
pub mod endpoints;
mod locks;
pub mod raw_capture;
pub mod session;
pub mod throttle;
pub mod to_core;

pub use endpoints::VisiblePuuid;
pub use session::{RiotError, RiotSession};
pub use throttle::Throttle;
