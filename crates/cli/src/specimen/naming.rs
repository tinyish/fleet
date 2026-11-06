//! Samples of a name that points at nothing a command takes, for the
//! specimen: for each set of kinds a command takes, from a fleet and from
//! outside every fleet. No `_` arm: a new refusal does not compile until it
//! has a sample here.

use fleet_core::status::{Naming, NamingKind, Takes};
use strum::IntoEnumIterator;

use super::samples::{APP, path};

/// Every sample of `kind`.
pub(super) fn samples(kind: NamingKind) -> Vec<Naming> {
    match kind {
        NamingKind::Unknown => Takes::iter()
            .flat_map(|takes| {
                [
                    Naming::Unknown {
                        name: "scrib".into(),
                        fleet: Some(path(APP)),
                        takes,
                    },
                    Naming::Unknown {
                        name: "s3f9a01c2d4".into(),
                        fleet: None,
                        takes,
                    },
                ]
            })
            .collect(),
    }
}
