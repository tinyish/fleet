//! Samples of where a fleet cannot go, for the specimen. No `_` arm: a new refusal does
//! not compile until it has a sample here.

use fleet_core::status::{Fleet, FleetKind};

use super::samples::{API, APP, SRC, WING, path};

/// Every sample of `kind`.
pub(super) fn samples(kind: FleetKind) -> Vec<Fleet> {
    use Fleet as F;
    use FleetKind as K;
    vec![match kind {
        K::DirectoryNotInRepo => F::DirectoryNotInRepo { path: path(WING) },
        K::DirectoryNotRepoRoot => F::DirectoryNotRepoRoot {
            path: path(SRC),
            root: path(APP),
        },
        K::ProjectNotInRepo => F::ProjectNotInRepo { path: path(WING) },
        K::ProjectNotRepoRoot => F::ProjectNotRepoRoot {
            path: path(SRC),
            root: path(APP),
        },
        K::ProjectIsFleetDirectory => F::ProjectIsFleetDirectory { path: path(APP) },
        K::WingmanInsideProject => F::WingmanInsideProject {
            directory: path(SRC),
            project: path(APP),
        },
        K::AlreadyHereServingOthers => F::AlreadyHereServingOthers {
            directory: path(WING),
            serving: vec![path(APP)],
            asked: vec![path(API)],
        },
        K::InsideFleet => F::InsideFleet {
            directory: path(SRC),
            fleet: path(APP),
        },
        K::ContainsFleet => F::ContainsFleet {
            directory: path("/code"),
            fleet: path(APP),
        },
        K::InsideServedProject => F::InsideServedProject {
            directory: path(SRC),
            project: path(APP),
            fleet: path(WING),
        },
        K::AlreadyServed => F::AlreadyServed {
            project: path(APP),
            fleet: path(WING),
        },
        K::AlsoClaimed => F::AlsoClaimed {
            project: path(API),
            fleet: path(WING),
        },
        K::NotFleetDirectory => F::NotFleetDirectory {
            directory: path(SRC),
            fleet: path(APP),
        },
    }]
}
