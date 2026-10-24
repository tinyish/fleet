//! The rules of tinyfleet, the protocol the CLI and the supervisor speak,
//! and the tool interfaces through which it reaches the outside world. No
//! I/O happens here except through those interfaces; the protocol reads and
//! writes only the stream it is handed.

pub mod config;
pub mod fleets;
pub mod paths;
pub mod protocol;
pub mod status;
pub mod tools;
