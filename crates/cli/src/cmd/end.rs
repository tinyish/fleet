//! `fleet end`: ends a seat's session (the seat comes back fresh while its
//! fleet is on) or a session by its id, dropping the messages held for it.
//! It never starts the supervisor.

use std::path::Path;

use fleet_core::protocol::{self, request};

use crate::client;
use crate::protocol::{Response, Status};

/// Ends `name`, a seat of the fleet here or a session's id.
pub(crate) fn run(state_dir: &Path, name: &str) -> Result<Response, Status> {
    let (supervisor, directory) = client::from_here(state_dir)?;
    let request = protocol::Request::End(request::End::Session {
        directory,
        name: name.to_owned(),
    });
    let ended = supervisor.ask(&request, |answer| match answer {
        protocol::Response::End(ended) => Some(ended),
        _ => None,
    })?;
    Ok(Response::Ended(ended))
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    #[test]
    fn end_takes_one_name() {
        let args = crate::Args::try_parse_from(["fleet", "end", "scribe"]);
        assert!(
            matches!(
                args.as_ref().map(|a| &a.request),
                Ok(crate::protocol::Request::End { name }) if name == "scribe"
            ),
            "{args:?}"
        );
        assert!(crate::Args::try_parse_from(["fleet", "end"]).is_err());
    }
}
