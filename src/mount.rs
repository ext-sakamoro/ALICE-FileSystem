//! Mount table (`MountEntry`).

use crate::inode::InodeId;

// Mount table
// ---------------------------------------------------------------------------

/// Represents a mounted sub-filesystem at a given path.
#[derive(Debug, Clone)]
pub struct MountEntry {
    pub mount_path: String,
    pub root_inode: InodeId,
    pub label: String,
}
