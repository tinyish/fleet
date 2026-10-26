//! Samples of one project that cannot be turned on or off, for the specimen. No `_` arm: a new refusal does
//! not compile until it has a sample here.

use fleet_core::status::{Project, ProjectKind};

use super::samples::{API, APP, SRC, WING, bad_config, path};

/// Every sample of `kind`.
pub(super) fn samples(kind: ProjectKind) -> Vec<Project> {
    use Project as P;
    use ProjectKind as K;
    vec![match kind {
        K::NotServed => P::NotServed {
            path: path("/code/blog"),
        },
        K::InsideProject => P::InsideProject {
            path: path(SRC),
            project: path(APP),
        },
        K::NoLongerServed => P::NoLongerServed {
            project: path(API),
            fleet: path(WING),
        },
        K::CannotTurnOn => P::CannotTurnOn {
            project: path(API),
            fleet: path(WING),
            why: Box::new(bad_config()),
        },
    }]
}
