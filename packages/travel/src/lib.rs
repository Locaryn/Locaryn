//! Travel mode: reaching your own machine from somewhere else.
//!
//! Optional, and deliberately not part of the enterprise story — a company
//! uses its local network or its own VPN. This is for the person whose
//! computer stays at home while they do not.
//!
//! Three pieces:
//! - [`providers`] opens an outbound tunnel through a relay,
//! - [`link`] produces the signed `lochor://` address the phone will read,
//! - [`qr`] turns that address into something a camera can see.

pub mod link;
pub mod providers;
pub mod qr;

pub use link::{sign, verify, LinkError, Mode, PairingLink};
pub use providers::{start, Provider, Tunnel, TunnelError};
