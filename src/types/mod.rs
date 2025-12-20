pub mod context;
pub mod dependency;
pub mod finding;
pub mod meta;
pub mod patch;
pub mod profile;
pub mod session_result;
pub mod workspace;

pub use dependency::{
    BlockType, DependencyProtocol, DependencySource, RuntimeDependency, is_dynamic_uri,
};
pub use patch::{FilePatch, PatchHunk, PatchRange, apply_file_patch, make_unified_diff};

pub use workspace::{AdvertisedProfile, MetaFile, MetaFileKind, WorkspaceDescriptor};

pub use profile::{FilePredicate, FileQueryHint, Profile};
