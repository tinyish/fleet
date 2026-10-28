//! The rules of tinyfleet, the protocol the CLI and the supervisor speak,
//! and the tool interfaces through which it reaches the outside world. No
//! I/O happens here except through those interfaces; the protocol reads and
//! writes only the stream it is handed.

pub mod adapters;
pub mod agent;
pub mod config;
pub mod fleets;
pub mod messages;
pub mod paths;
pub mod protocol;
pub mod record;
pub mod seats;
pub mod status;
pub mod tools;
pub mod waiting;
