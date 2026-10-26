//! Samples of a path or a config file that cannot be used, for the specimen. No `_` arm: a new refusal does
//! not compile until it has a sample here.

use fleet_core::status::{File, FileKind};

use super::samples::{API, WEB, denied, path};

/// Every sample of `kind`.
pub(super) fn samples(kind: FileKind) -> Vec<File> {
    use File as F;
    use FileKind as K;
    vec![match kind {
        K::Missing => F::Missing { path: path(API) },
        K::Unreadable => F::Unreadable {
            path: path(API),
            error: denied().to_string(),
        },
        K::ConfigDiffers => F::ConfigDiffers {
            path: path("/code/wing/fleet.toml"),
            listed: vec![path(API), path(WEB)],
        },
        K::CannotWrite => F::CannotWrite {
            path: path("/code/app/fleet.toml"),
            error: denied().to_string(),
        },
        K::BadConfig => bad_config_sample(),
    }]
}

/// A config that does not read, where the parser knows: the one sample of a
/// broken config, which other refusals carry as their reason too.
pub(super) fn bad_config_sample() -> File {
    File::BadConfig {
        path: path("/code/wing/fleet.toml"),
        line: Some(1),
        column: Some(13),
        error: "unclosed array, expected `]`".into(),
    }
}
