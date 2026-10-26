//! Every refusal the supervisor can answer with, as samples for the
//! specimen, gathered from one file per area. Every match here and there has
//! no `_` arm, so a new refusal does not compile until it has a sample.

use fleet_core::protocol;
use fleet_core::status::{AdapterKind, FileKind, FleetKind, ProjectKind, SessionKind, StatusKind};
use strum::IntoEnumIterator;

/// Every sample of `kind`, each with its label.
pub(super) fn refusals(kind: StatusKind) -> Vec<(String, protocol::Status)> {
    match kind {
        StatusKind::Fleet => each(FleetKind::iter(), super::fleet::samples),
        StatusKind::Project => each(ProjectKind::iter(), super::project::samples),
        StatusKind::File => each(FileKind::iter(), super::file::samples),
        StatusKind::Session => each(SessionKind::iter(), super::session::samples),
        StatusKind::Adapter => each(AdapterKind::iter(), super::adapter::samples),
        StatusKind::Malformed => vec![(
            "Malformed".into(),
            protocol::Status::Malformed {
                error: "expected value at line 1 column 1".into(),
            },
        )],
    }
}

/// The samples of every kind of one area, each labelled with its kind.
fn each<K: std::fmt::Debug + Copy, R: Into<protocol::Status>>(
    kinds: impl Iterator<Item = K>,
    samples: impl Fn(K) -> Vec<R>,
) -> Vec<(String, protocol::Status)> {
    kinds
        .flat_map(|k| {
            samples(k)
                .into_iter()
                .map(move |r| (format!("{k:?}"), r.into()))
        })
        .collect()
}
