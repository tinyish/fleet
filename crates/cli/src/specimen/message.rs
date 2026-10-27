//! Samples of what goes wrong sending a message, for the specimen. No `_`
//! arm: a new refusal does not compile until it has a sample here.

use fleet_core::messages::MAX_TEXT_BYTES;
use fleet_core::status::{Message, MessageKind};

use super::samples::{APP, path};

/// Every sample of `kind`.
pub(super) fn samples(kind: MessageKind) -> Vec<Message> {
    use Message as M;
    use MessageKind as K;
    match kind {
        K::NoSuchRecipient => vec![
            M::NoSuchRecipient {
                to: "scribe".into(),
                fleet: Some(path(APP)),
            },
            M::NoSuchRecipient {
                to: "s3f9a01c2d4".into(),
                fleet: None,
            },
        ],
        K::SessionGone => vec![M::SessionGone {
            session: "s3f9a01c2d4".into(),
        }],
        K::EmptyText => vec![M::EmptyText],
        K::TooLong => vec![M::TooLong {
            bytes: 40_960,
            limit: MAX_TEXT_BYTES,
        }],
    }
}
