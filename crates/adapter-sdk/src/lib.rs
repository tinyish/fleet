//! tinyfleet's adapter protocols, and a kit for writing an adapter in Rust.
//!
//! An adapter is a program tinyfleet runs once per request as
//! `<adapter> <verb>`, with the request as one JSON object on stdin. It
//! answers with one JSON object on stdout and exits: 0 with an answer, 1
//! when it cannot answer (why on stderr), 2 for a verb it does not know.
//! Every adapter, whatever it adapts, answers `protocol` with [`Speaks`].
//! [`agent`] is the protocol for agents. Behind the `kit` feature, `kit`
//! holds the run loop, helpers, a test kit and the conformance checks.

pub mod agent;
#[cfg(feature = "kit")]
pub mod kit;

use serde::{Deserialize, Serialize};

/// What every adapter answers to `protocol`: the number of the protocol it
/// speaks for its kind, such as [`agent::PROTOCOL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speaks {
    /// The protocol's number.
    pub protocol: u32,
}
