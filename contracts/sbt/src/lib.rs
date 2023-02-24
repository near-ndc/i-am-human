mod events;
mod metadata;

pub use crate::events::*;
pub use crate::metadata::*;

pub type TokenId = u64;

/// This spec can be treated like a version of the standard.
pub const METADATA_SPEC: &str = "1.0.0";
/// This is the name of the SBT standard we're using
pub const SBT_STANDARD_NAME: &str = "nep-393";
