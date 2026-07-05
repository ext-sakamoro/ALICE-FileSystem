//! `FsError` + `FsResult` — filesystem error types.

use std::fmt;

// Errors
// ---------------------------------------------------------------------------

/// All possible filesystem errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    NotEmpty,
    PermissionDenied,
    InvalidPath,
    InvalidFileDescriptor,
    SymlinkLoop,
    NotASymlink,
    MountPointNotEmpty,
    MountPointNotFound,
    CrossMount,
    BufferFull,
    EndOfFile,
    ReadOnly,
    WriteOnly,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NotFound => "not found",
            Self::AlreadyExists => "already exists",
            Self::NotADirectory => "not a directory",
            Self::IsADirectory => "is a directory",
            Self::NotEmpty => "directory not empty",
            Self::PermissionDenied => "permission denied",
            Self::InvalidPath => "invalid path",
            Self::InvalidFileDescriptor => "invalid file descriptor",
            Self::SymlinkLoop => "too many symlink levels",
            Self::NotASymlink => "not a symlink",
            Self::MountPointNotEmpty => "mount point not empty",
            Self::MountPointNotFound => "mount point not found",
            Self::CrossMount => "cross-mount operation",
            Self::BufferFull => "buffer full",
            Self::EndOfFile => "end of file",
            Self::ReadOnly => "file descriptor is read-only",
            Self::WriteOnly => "file descriptor is write-only",
        };
        f.write_str(msg)
    }
}

pub type FsResult<T> = Result<T, FsError>;
