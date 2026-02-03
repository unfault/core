pub mod context;
pub mod dependency;
pub mod finding;
pub mod meta;
pub mod patch;
pub mod profile;
pub mod session_result;
pub mod workspace;

pub use dependency::{
    is_dynamic_uri, BlockType, DependencyProtocol, DependencySource, RuntimeDependency,
};
pub use patch::{apply_file_patch, make_unified_diff, FilePatch, PatchHunk, PatchRange};

pub use workspace::{AdvertisedProfile, MetaFile, MetaFileKind, WorkspaceDescriptor};

pub use profile::{FilePredicate, FileQueryHint, Profile};
