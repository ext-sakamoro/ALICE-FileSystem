//! `FileDescriptor` + `OpenMode` — file open handles.

use crate::inode::InodeId;

// File descriptor & open mode
// ---------------------------------------------------------------------------

/// How a file was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Read,
    Write,
    ReadWrite,
    Append,
}

/// A file descriptor referencing an open file.
#[derive(Debug, Clone)]
pub struct FileDescriptor {
    pub fd: u32,
    pub inode_id: InodeId,
    pub mode: OpenMode,
    pub offset: u64,
}
