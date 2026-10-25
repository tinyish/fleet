//! tinyfleet's adapter protocols, and a kit for writing an adapter in Rust.
//!
//! An adapter is a program tinyfleet runs once per request as
//! `<adapter> <verb>`, with the request as one JSON object on stdin. It
//! answers with one JSON object on stdout and exits: 0 with an answer, 1
//! when it cannot answer (why on stderr), 2 for a verb it does not know.
//! [`agent`] is the protocol for agents. Behind the `kit` feature, `kit`
//! holds the run loop, helpers, a test kit and the conformance checks.

pub mod agent;
#[cfg(feature = "kit")]
pub mod kit;
