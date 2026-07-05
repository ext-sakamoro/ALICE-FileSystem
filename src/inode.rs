//! `Inode` + `InodeKind` — file/directory/symlink inode.

use crate::permissions::Permissions;
use std::collections::HashMap;

// Inode & node types
// ---------------------------------------------------------------------------

pub type InodeId = u64;

/// The kind of data stored in an inode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InodeKind {
    File { data: Vec<u8> },
    Directory { children: HashMap<String, InodeId> },
    Symlink { target: String },
}

/// A single inode in the virtual filesystem.
#[derive(Debug, Clone)]
pub struct Inode {
    pub id: InodeId,
    pub kind: InodeKind,
    pub permissions: Permissions,
    pub link_count: u32,
    pub size: u64,
}

impl Inode {
    pub fn new_dir(id: InodeId, perms: Permissions) -> Self {
        Self {
            id,
            kind: InodeKind::Directory {
                children: HashMap::new(),
            },
            permissions: perms,
            link_count: 2, // . and parent
            size: 0,
        }
    }

    pub const fn new_file(id: InodeId, perms: Permissions) -> Self {
        Self {
            id,
            kind: InodeKind::File { data: Vec::new() },
            permissions: perms,
            link_count: 1,
            size: 0,
        }
    }

    pub const fn new_symlink(id: InodeId, target: String, perms: Permissions) -> Self {
        let size = target.len() as u64;
        Self {
            id,
            kind: InodeKind::Symlink { target },
            permissions: perms,
            link_count: 1,
            size,
        }
    }

    #[must_use]
    pub const fn is_dir(&self) -> bool {
        matches!(self.kind, InodeKind::Directory { .. })
    }

    #[must_use]
    pub const fn is_file(&self) -> bool {
        matches!(self.kind, InodeKind::File { .. })
    }

    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        matches!(self.kind, InodeKind::Symlink { .. })
    }
}
