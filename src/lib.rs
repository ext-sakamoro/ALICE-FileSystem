#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions)]

//! ALICE-FileSystem: Pure Rust virtual filesystem.
//!
//! Provides inodes, directory tree, file permissions (rwx), symlinks,
//! mounting, path resolution, file descriptors, and buffered I/O abstraction.

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
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

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

/// Unix-style permission bits for owner (simplified: single user model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permissions {
    #[must_use]
    pub const fn new(read: bool, write: bool, execute: bool) -> Self {
        Self {
            read,
            write,
            execute,
        }
    }

    /// `rwx` -- full permissions.
    #[must_use]
    pub const fn all() -> Self {
        Self::new(true, true, true)
    }

    /// `r--` -- read only.
    #[must_use]
    pub const fn read_only() -> Self {
        Self::new(true, false, false)
    }

    /// `rw-` -- read/write.
    #[must_use]
    pub const fn read_write() -> Self {
        Self::new(true, true, false)
    }

    /// No permissions.
    #[must_use]
    pub const fn none() -> Self {
        Self::new(false, false, false)
    }

    /// Numeric representation (octal-style single digit 0-7).
    #[must_use]
    pub const fn as_octal(&self) -> u8 {
        let mut v = 0u8;
        if self.read {
            v += 4;
        }
        if self.write {
            v += 2;
        }
        if self.execute {
            v += 1;
        }
        v
    }

    /// Build from octal digit (0-7).
    #[must_use]
    pub const fn from_octal(val: u8) -> Self {
        Self {
            read: val & 4 != 0,
            write: val & 2 != 0,
            execute: val & 1 != 0,
        }
    }
}

impl fmt::Display for Permissions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            if self.read { 'r' } else { '-' },
            if self.write { 'w' } else { '-' },
            if self.execute { 'x' } else { '-' },
        )
    }
}

// ---------------------------------------------------------------------------
// Inode & node types
// ---------------------------------------------------------------------------

type InodeId = u64;

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
    fn new_dir(id: InodeId, perms: Permissions) -> Self {
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

    const fn new_file(id: InodeId, perms: Permissions) -> Self {
        Self {
            id,
            kind: InodeKind::File { data: Vec::new() },
            permissions: perms,
            link_count: 1,
            size: 0,
        }
    }

    const fn new_symlink(id: InodeId, target: String, perms: Permissions) -> Self {
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

// ---------------------------------------------------------------------------
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

// ---------------------------------------------------------------------------
// Buffered I/O
// ---------------------------------------------------------------------------

/// A simple buffered writer that batches writes.
#[derive(Debug)]
pub struct BufWriter {
    fd: u32,
    buffer: Vec<u8>,
    capacity: usize,
}

impl BufWriter {
    /// Create a new buffered writer for the given file descriptor.
    #[must_use]
    pub fn new(fd: u32, capacity: usize) -> Self {
        Self {
            fd,
            buffer: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Write data into the internal buffer, flushing to `fs` if the buffer
    /// would exceed its capacity.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying write to the filesystem fails.
    pub fn write(&mut self, fs: &mut VirtualFs, data: &[u8]) -> FsResult<()> {
        for &byte in data {
            self.buffer.push(byte);
            if self.buffer.len() >= self.capacity {
                self.flush(fs)?;
            }
        }
        Ok(())
    }

    /// Flush any buffered data to the filesystem.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying write fails.
    pub fn flush(&mut self, fs: &mut VirtualFs) -> FsResult<()> {
        if !self.buffer.is_empty() {
            let data = std::mem::take(&mut self.buffer);
            fs.write(self.fd, &data)?;
        }
        Ok(())
    }

    /// Return how many bytes are currently buffered.
    #[must_use]
    pub const fn buffered_len(&self) -> usize {
        self.buffer.len()
    }
}

/// A simple buffered reader that reads in chunks.
#[derive(Debug)]
pub struct BufReader {
    fd: u32,
    buffer: Vec<u8>,
    pos: usize,
    chunk_size: usize,
}

impl BufReader {
    /// Create a new buffered reader for the given file descriptor.
    #[must_use]
    pub const fn new(fd: u32, chunk_size: usize) -> Self {
        Self {
            fd,
            buffer: Vec::new(),
            pos: 0,
            chunk_size,
        }
    }

    /// Read up to `len` bytes, refilling the internal buffer as needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying read fails.
    pub fn read(&mut self, fs: &mut VirtualFs, len: usize) -> FsResult<Vec<u8>> {
        let mut result = Vec::with_capacity(len);
        let mut remaining = len;

        while remaining > 0 {
            if self.pos >= self.buffer.len() {
                // refill
                self.buffer = fs.read(self.fd, self.chunk_size)?;
                self.pos = 0;
                if self.buffer.is_empty() {
                    break; // EOF
                }
            }
            let available = self.buffer.len() - self.pos;
            let take = remaining.min(available);
            result.extend_from_slice(&self.buffer[self.pos..self.pos + take]);
            self.pos += take;
            remaining -= take;
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Mount table
// ---------------------------------------------------------------------------

/// Represents a mounted sub-filesystem at a given path.
#[derive(Debug, Clone)]
pub struct MountEntry {
    pub mount_path: String,
    pub root_inode: InodeId,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Virtual Filesystem
// ---------------------------------------------------------------------------

/// The main virtual filesystem struct.
#[derive(Debug)]
pub struct VirtualFs {
    inodes: HashMap<InodeId, Inode>,
    next_inode: InodeId,
    root_inode: InodeId,
    fds: HashMap<u32, FileDescriptor>,
    next_fd: u32,
    mounts: Vec<MountEntry>,
    max_symlink_depth: u32,
}

impl Default for VirtualFs {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to insert a child entry into the parent directory inode.
///
/// # Panics
///
/// Panics if `parent_id` does not exist in `inodes` (internal invariant).
fn insert_child(
    inodes: &mut HashMap<InodeId, Inode>,
    parent_id: InodeId,
    name: String,
    child_id: InodeId,
) {
    let parent = inodes.get_mut(&parent_id).expect("parent inode must exist");
    if let InodeKind::Directory { children } = &mut parent.kind {
        children.insert(name, child_id);
    }
}

/// Helper to remove a child entry from the parent directory inode.
///
/// # Panics
///
/// Panics if `parent_id` does not exist in `inodes` (internal invariant).
fn remove_child(inodes: &mut HashMap<InodeId, Inode>, parent_id: InodeId, name: &str) {
    let parent = inodes.get_mut(&parent_id).expect("parent inode must exist");
    if let InodeKind::Directory { children } = &mut parent.kind {
        children.remove(name);
    }
}

impl VirtualFs {
    // ---- construction ------------------------------------------------------

    /// Create a new virtual filesystem with a root directory.
    #[must_use]
    pub fn new() -> Self {
        let root_id: InodeId = 1;
        let root = Inode::new_dir(root_id, Permissions::all());
        let mut inodes = HashMap::new();
        inodes.insert(root_id, root);

        Self {
            inodes,
            next_inode: 2,
            root_inode: root_id,
            fds: HashMap::new(),
            next_fd: 0,
            mounts: Vec::new(),
            max_symlink_depth: 40,
        }
    }

    const fn alloc_inode(&mut self) -> InodeId {
        let id = self.next_inode;
        self.next_inode += 1;
        id
    }

    const fn alloc_fd(&mut self) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        fd
    }

    // ---- path helpers ------------------------------------------------------

    /// Normalise an absolute path into components.
    fn split_path(path: &str) -> FsResult<Vec<&str>> {
        let path = path.trim();
        if !path.starts_with('/') {
            return Err(FsError::InvalidPath);
        }
        Ok(path.split('/').filter(|s| !s.is_empty()).collect())
    }

    /// Resolve a path to its inode, following symlinks up to the depth limit.
    fn resolve_path_inner(&self, path: &str, follow_last: bool, depth: u32) -> FsResult<InodeId> {
        if depth > self.max_symlink_depth {
            return Err(FsError::SymlinkLoop);
        }
        let components = Self::split_path(path)?;
        let mut current = self.root_inode;

        // Check mount points -- longest prefix match
        current = self.resolve_mount_root(path, current);

        let len = components.len();
        for (i, comp) in components.iter().enumerate() {
            let is_last = i + 1 == len;

            // Dereference current if symlink
            let inode = self.inodes.get(&current).ok_or(FsError::NotFound)?;
            if let InodeKind::Symlink { target } = &inode.kind {
                let resolved = self.resolve_symlink_target(target, depth)?;
                current = resolved;
            }

            let inode = self.inodes.get(&current).ok_or(FsError::NotFound)?;
            match &inode.kind {
                InodeKind::Directory { children } => {
                    if *comp != "." && *comp != ".." {
                        current = *children.get(*comp).ok_or(FsError::NotFound)?;
                    }
                    // "." stays, ".." stays (simplified: root's ".." is root)
                }
                _ => {
                    if !is_last {
                        return Err(FsError::NotADirectory);
                    }
                }
            }

            // Follow symlink at last component if requested
            if is_last {
                let node = self.inodes.get(&current).ok_or(FsError::NotFound)?;
                if follow_last {
                    if let InodeKind::Symlink { target } = &node.kind {
                        return self.resolve_symlink_target(target, depth);
                    }
                }
            }
        }

        Ok(current)
    }

    fn resolve_symlink_target(&self, target: &str, depth: u32) -> FsResult<InodeId> {
        self.resolve_path_inner(target, true, depth + 1)
    }

    fn resolve_mount_root(&self, path: &str, default: InodeId) -> InodeId {
        let mut best_len = 0;
        let mut best_root = default;
        for m in &self.mounts {
            let mp = &m.mount_path;
            if path.starts_with(mp.as_str())
                && mp.len() > best_len
                && (path.len() == mp.len() || path.as_bytes().get(mp.len()) == Some(&b'/'))
            {
                best_len = mp.len();
                best_root = m.root_inode;
            }
        }
        best_root
    }

    /// Resolve path following symlinks on the last component.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or not found.
    pub fn resolve_path(&self, path: &str) -> FsResult<InodeId> {
        self.resolve_path_inner(path, true, 0)
    }

    /// Resolve path WITHOUT following the last symlink.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or not found.
    pub fn resolve_path_no_follow(&self, path: &str) -> FsResult<InodeId> {
        self.resolve_path_inner(path, false, 0)
    }

    /// Split a path into (parent dir path, basename).
    fn parent_and_name(path: &str) -> FsResult<(String, String)> {
        let path = path.trim().trim_end_matches('/');
        if path.is_empty() || path == "/" {
            return Err(FsError::InvalidPath);
        }
        if let Some(pos) = path.rfind('/') {
            let parent = if pos == 0 {
                "/".to_string()
            } else {
                path[..pos].to_string()
            };
            let name = path[pos + 1..].to_string();
            if name.is_empty() {
                return Err(FsError::InvalidPath);
            }
            Ok((parent, name))
        } else {
            Err(FsError::InvalidPath)
        }
    }

    // ---- helpers for checked parent operations -----------------------------

    /// Validate the parent exists, is a writable directory, and the child
    /// name does not yet exist. Returns `(parent_id, name)`.
    fn validate_parent_for_create(&self, path: &str) -> FsResult<(InodeId, String)> {
        let (parent_path, name) = Self::parent_and_name(path)?;
        let parent_id = self.resolve_path(&parent_path)?;

        let parent = self.inodes.get(&parent_id).ok_or(FsError::NotFound)?;
        if !parent.permissions.write {
            return Err(FsError::PermissionDenied);
        }
        let InodeKind::Directory { children } = &parent.kind else {
            return Err(FsError::NotADirectory);
        };
        if children.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }

        Ok((parent_id, name))
    }

    /// Validate the parent exists, is a writable directory, and the child
    /// name exists. Returns `(parent_id, child_name, child_id)`.
    fn validate_parent_for_remove(&self, path: &str) -> FsResult<(InodeId, String, InodeId)> {
        let (parent_path, name) = Self::parent_and_name(path)?;
        let parent_id = self.resolve_path(&parent_path)?;

        let parent = self.inodes.get(&parent_id).ok_or(FsError::NotFound)?;
        if !parent.permissions.write {
            return Err(FsError::PermissionDenied);
        }

        let InodeKind::Directory { children } = &parent.kind else {
            return Err(FsError::NotADirectory);
        };
        let child_id = *children.get(&name).ok_or(FsError::NotFound)?;

        Ok((parent_id, name, child_id))
    }

    // ---- queries ------------------------------------------------------------

    /// Get a reference to an inode by its ID.
    #[must_use]
    pub fn get_inode(&self, id: InodeId) -> Option<&Inode> {
        self.inodes.get(&id)
    }

    /// Get a reference to the inode at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the path cannot be resolved.
    pub fn stat(&self, path: &str) -> FsResult<&Inode> {
        let id = self.resolve_path(path)?;
        self.inodes.get(&id).ok_or(FsError::NotFound)
    }

    /// Same as `stat` but does not follow the final symlink.
    ///
    /// # Errors
    ///
    /// Returns an error if the path cannot be resolved.
    pub fn lstat(&self, path: &str) -> FsResult<&Inode> {
        let id = self.resolve_path_no_follow(path)?;
        self.inodes.get(&id).ok_or(FsError::NotFound)
    }

    /// List names in a directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is not a directory.
    pub fn list_dir(&self, path: &str) -> FsResult<Vec<String>> {
        let id = self.resolve_path(path)?;
        let inode = self.inodes.get(&id).ok_or(FsError::NotFound)?;
        if !inode.permissions.read {
            return Err(FsError::PermissionDenied);
        }
        match &inode.kind {
            InodeKind::Directory { children } => {
                let mut names: Vec<String> = children.keys().cloned().collect();
                names.sort();
                Ok(names)
            }
            _ => Err(FsError::NotADirectory),
        }
    }

    /// Return total number of inodes.
    #[must_use]
    pub fn inode_count(&self) -> usize {
        self.inodes.len()
    }

    // ---- creation -----------------------------------------------------------

    /// Create a directory at the given absolute path.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent doesn't exist, is not a directory,
    /// or the name already exists.
    pub fn mkdir(&mut self, path: &str) -> FsResult<InodeId> {
        self.mkdir_with_perms(path, Permissions::all())
    }

    /// Create a directory with explicit permissions.
    ///
    /// # Errors
    ///
    /// Returns an error on invalid parent or existing name.
    ///
    /// # Panics
    ///
    /// Panics if internal inode invariants are violated (should never happen).
    pub fn mkdir_with_perms(&mut self, path: &str, perms: Permissions) -> FsResult<InodeId> {
        let (parent_id, name) = self.validate_parent_for_create(path)?;

        let new_id = self.alloc_inode();
        let new_dir = Inode::new_dir(new_id, perms);
        self.inodes.insert(new_id, new_dir);

        insert_child(&mut self.inodes, parent_id, name, new_id);
        let parent = self.inodes.get_mut(&parent_id).expect("parent must exist");
        parent.link_count += 1;

        Ok(new_id)
    }

    /// Create an empty file at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if parent is invalid or name exists.
    pub fn create_file(&mut self, path: &str) -> FsResult<InodeId> {
        self.create_file_with_perms(path, Permissions::read_write())
    }

    /// Create an empty file with explicit permissions.
    ///
    /// # Errors
    ///
    /// Returns an error on invalid parent or existing name.
    pub fn create_file_with_perms(&mut self, path: &str, perms: Permissions) -> FsResult<InodeId> {
        let (parent_id, name) = self.validate_parent_for_create(path)?;

        let new_id = self.alloc_inode();
        let new_file = Inode::new_file(new_id, perms);
        self.inodes.insert(new_id, new_file);

        insert_child(&mut self.inodes, parent_id, name, new_id);

        Ok(new_id)
    }

    /// Create a symlink at `link_path` pointing to `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if parent is invalid or name exists.
    pub fn create_symlink(&mut self, link_path: &str, target: &str) -> FsResult<InodeId> {
        let (parent_id, name) = self.validate_parent_for_create(link_path)?;

        let new_id = self.alloc_inode();
        let symlink = Inode::new_symlink(new_id, target.to_string(), Permissions::all());
        self.inodes.insert(new_id, symlink);

        insert_child(&mut self.inodes, parent_id, name, new_id);

        Ok(new_id)
    }

    /// Read the target of a symlink without following it.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is not a symlink.
    pub fn read_link(&self, path: &str) -> FsResult<String> {
        let id = self.resolve_path_no_follow(path)?;
        let inode = self.inodes.get(&id).ok_or(FsError::NotFound)?;
        match &inode.kind {
            InodeKind::Symlink { target } => Ok(target.clone()),
            _ => Err(FsError::NotASymlink),
        }
    }

    // ---- removal ------------------------------------------------------------

    /// Remove a file or symlink. Fails on directories.
    ///
    /// # Errors
    ///
    /// Returns an error if the target is a directory or doesn't exist.
    ///
    /// # Panics
    ///
    /// Panics if internal inode invariants are violated (should never happen).
    pub fn unlink(&mut self, path: &str) -> FsResult<()> {
        let (parent_id, name, child_id) = self.validate_parent_for_remove(path)?;

        let child = self.inodes.get(&child_id).ok_or(FsError::NotFound)?;
        if child.is_dir() {
            return Err(FsError::IsADirectory);
        }

        remove_child(&mut self.inodes, parent_id, &name);

        // Decrement link count; remove inode if zero
        let child_mut = self.inodes.get_mut(&child_id).expect("child must exist");
        child_mut.link_count = child_mut.link_count.saturating_sub(1);
        if child_mut.link_count == 0 {
            self.inodes.remove(&child_id);
        }

        Ok(())
    }

    /// Remove an empty directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the target is not a directory, not empty, or the
    /// path is the root.
    ///
    /// # Panics
    ///
    /// Panics if internal inode invariants are violated (should never happen).
    pub fn rmdir(&mut self, path: &str) -> FsResult<()> {
        let (parent_id, name, child_id) = self.validate_parent_for_remove(path)?;

        let child = self.inodes.get(&child_id).ok_or(FsError::NotFound)?;
        match &child.kind {
            InodeKind::Directory { children } => {
                if !children.is_empty() {
                    return Err(FsError::NotEmpty);
                }
            }
            _ => return Err(FsError::NotADirectory),
        }

        remove_child(&mut self.inodes, parent_id, &name);
        let parent = self.inodes.get_mut(&parent_id).expect("parent must exist");
        parent.link_count = parent.link_count.saturating_sub(1);

        self.inodes.remove(&child_id);
        Ok(())
    }

    // ---- direct read/write (no fd) -----------------------------------------

    /// Write raw bytes directly to a file inode (overwrites).
    ///
    /// # Errors
    ///
    /// Returns an error if the path doesn't point to a writable file.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> FsResult<()> {
        let id = self.resolve_path(path)?;
        let inode = self.inodes.get_mut(&id).ok_or(FsError::NotFound)?;
        if !inode.permissions.write {
            return Err(FsError::PermissionDenied);
        }
        match &mut inode.kind {
            InodeKind::File { data: file_data } => {
                *file_data = data.to_vec();
                inode.size = data.len() as u64;
                Ok(())
            }
            _ => Err(FsError::IsADirectory),
        }
    }

    /// Read all bytes from a file inode.
    ///
    /// # Errors
    ///
    /// Returns an error if the path doesn't point to a readable file.
    pub fn read_file(&self, path: &str) -> FsResult<Vec<u8>> {
        let id = self.resolve_path(path)?;
        let inode = self.inodes.get(&id).ok_or(FsError::NotFound)?;
        if !inode.permissions.read {
            return Err(FsError::PermissionDenied);
        }
        match &inode.kind {
            InodeKind::File { data } => Ok(data.clone()),
            _ => Err(FsError::IsADirectory),
        }
    }

    // ---- file descriptors ---------------------------------------------------

    /// Open a file and return a file descriptor number.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist or permissions don't
    /// match the requested mode.
    pub fn open(&mut self, path: &str, mode: OpenMode) -> FsResult<u32> {
        let id = self.resolve_path(path)?;
        let inode = self.inodes.get(&id).ok_or(FsError::NotFound)?;

        if inode.is_dir() {
            return Err(FsError::IsADirectory);
        }

        // Permission check
        match mode {
            OpenMode::Read => {
                if !inode.permissions.read {
                    return Err(FsError::PermissionDenied);
                }
            }
            OpenMode::Write | OpenMode::Append => {
                if !inode.permissions.write {
                    return Err(FsError::PermissionDenied);
                }
            }
            OpenMode::ReadWrite => {
                if !inode.permissions.read || !inode.permissions.write {
                    return Err(FsError::PermissionDenied);
                }
            }
        }

        let file_size = inode.size;

        let fd_num = self.alloc_fd();
        let offset = if mode == OpenMode::Append {
            file_size
        } else {
            0
        };

        self.fds.insert(
            fd_num,
            FileDescriptor {
                fd: fd_num,
                inode_id: id,
                mode,
                offset,
            },
        );

        Ok(fd_num)
    }

    /// Close a file descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error if the fd is invalid.
    pub fn close(&mut self, fd: u32) -> FsResult<()> {
        self.fds
            .remove(&fd)
            .map(|_| ())
            .ok_or(FsError::InvalidFileDescriptor)
    }

    /// Read up to `len` bytes from an open file descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error if the fd is invalid, not readable, or the inode is
    /// not a file.
    ///
    /// # Panics
    ///
    /// Panics if internal fd/inode invariants are violated (should never happen).
    #[allow(clippy::cast_possible_truncation)]
    pub fn read(&mut self, fd: u32, len: usize) -> FsResult<Vec<u8>> {
        let fd_entry = self.fds.get(&fd).ok_or(FsError::InvalidFileDescriptor)?;
        if fd_entry.mode == OpenMode::Write || fd_entry.mode == OpenMode::Append {
            return Err(FsError::WriteOnly);
        }
        let inode_id = fd_entry.inode_id;
        let offset = fd_entry.offset as usize;

        let inode = self.inodes.get(&inode_id).ok_or(FsError::NotFound)?;
        let InodeKind::File { data } = &inode.kind else {
            return Err(FsError::IsADirectory);
        };

        let end = data.len().min(offset + len);
        let result = if offset < data.len() {
            data[offset..end].to_vec()
        } else {
            Vec::new()
        };

        // Advance offset
        let fd_entry = self.fds.get_mut(&fd).expect("fd was just validated");
        fd_entry.offset = end as u64;

        Ok(result)
    }

    /// Write bytes through a file descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error if the fd is invalid or read-only.
    ///
    /// # Panics
    ///
    /// Panics if internal fd/inode invariants are violated (should never happen).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write(&mut self, fd: u32, buf: &[u8]) -> FsResult<usize> {
        let fd_entry = self.fds.get(&fd).ok_or(FsError::InvalidFileDescriptor)?;
        if fd_entry.mode == OpenMode::Read {
            return Err(FsError::ReadOnly);
        }
        let inode_id = fd_entry.inode_id;
        let offset = fd_entry.offset as usize;

        let inode = self.inodes.get_mut(&inode_id).ok_or(FsError::NotFound)?;
        let InodeKind::File { data } = &mut inode.kind else {
            return Err(FsError::IsADirectory);
        };

        // Extend if necessary
        let needed = offset + buf.len();
        if needed > data.len() {
            data.resize(needed, 0);
        }
        data[offset..offset + buf.len()].copy_from_slice(buf);

        inode.size = data.len() as u64;

        // Advance offset
        let fd_entry = self.fds.get_mut(&fd).expect("fd was just validated");
        fd_entry.offset = (offset + buf.len()) as u64;

        Ok(buf.len())
    }

    /// Seek a file descriptor to a given offset.
    ///
    /// # Errors
    ///
    /// Returns an error if the fd is invalid.
    pub fn seek(&mut self, fd: u32, offset: u64) -> FsResult<()> {
        let fd_entry = self
            .fds
            .get_mut(&fd)
            .ok_or(FsError::InvalidFileDescriptor)?;
        fd_entry.offset = offset;
        Ok(())
    }

    /// Get current offset of a file descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error if the fd is invalid.
    pub fn tell(&self, fd: u32) -> FsResult<u64> {
        let fd_entry = self.fds.get(&fd).ok_or(FsError::InvalidFileDescriptor)?;
        Ok(fd_entry.offset)
    }

    // ---- permissions --------------------------------------------------------

    /// Change permissions of the inode at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the path doesn't exist.
    pub fn chmod(&mut self, path: &str, perms: Permissions) -> FsResult<()> {
        let id = self.resolve_path(path)?;
        let inode = self.inodes.get_mut(&id).ok_or(FsError::NotFound)?;
        inode.permissions = perms;
        Ok(())
    }

    // ---- mounting -----------------------------------------------------------

    /// Mount a new empty filesystem at the given path. The path must point
    /// to an existing empty directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the mount point doesn't exist, is not a directory,
    /// or is not empty.
    pub fn mount(&mut self, mount_path: &str, label: &str) -> FsResult<()> {
        let id = self.resolve_path(mount_path)?;
        let inode = self.inodes.get(&id).ok_or(FsError::NotFound)?;
        match &inode.kind {
            InodeKind::Directory { children } => {
                if !children.is_empty() {
                    return Err(FsError::MountPointNotEmpty);
                }
            }
            _ => return Err(FsError::NotADirectory),
        }

        // Create a new root inode for the mounted FS
        let mount_root_id = self.alloc_inode();
        let mount_root = Inode::new_dir(mount_root_id, Permissions::all());
        self.inodes.insert(mount_root_id, mount_root);

        self.mounts.push(MountEntry {
            mount_path: mount_path.to_string(),
            root_inode: mount_root_id,
            label: label.to_string(),
        });

        Ok(())
    }

    /// Unmount the filesystem at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if no filesystem is mounted there.
    pub fn unmount(&mut self, mount_path: &str) -> FsResult<()> {
        let idx = self
            .mounts
            .iter()
            .position(|m| m.mount_path == mount_path)
            .ok_or(FsError::MountPointNotFound)?;
        self.mounts.remove(idx);
        Ok(())
    }

    /// List current mounts.
    #[must_use]
    pub fn list_mounts(&self) -> &[MountEntry] {
        &self.mounts
    }

    // ---- rename -------------------------------------------------------------

    /// Rename / move a file or directory.
    ///
    /// # Errors
    ///
    /// Returns an error if source doesn't exist or destination parent is
    /// invalid.
    pub fn rename(&mut self, old_path: &str, new_path: &str) -> FsResult<()> {
        let (old_parent_path, old_name) = Self::parent_and_name(old_path)?;
        let (new_parent_path, new_name) = Self::parent_and_name(new_path)?;

        let old_parent_id = self.resolve_path(&old_parent_path)?;
        let new_parent_id = self.resolve_path(&new_parent_path)?;

        // Get child id from old parent
        let old_parent = self.inodes.get(&old_parent_id).ok_or(FsError::NotFound)?;
        let child_id = match &old_parent.kind {
            InodeKind::Directory { children } => {
                *children.get(&old_name).ok_or(FsError::NotFound)?
            }
            _ => return Err(FsError::NotADirectory),
        };

        // Check new parent is dir and new name doesn't already exist
        let new_parent = self.inodes.get(&new_parent_id).ok_or(FsError::NotFound)?;
        match &new_parent.kind {
            InodeKind::Directory { children } => {
                if children.contains_key(&new_name) {
                    return Err(FsError::AlreadyExists);
                }
            }
            _ => return Err(FsError::NotADirectory),
        }

        // Remove from old parent, add to new parent
        remove_child(&mut self.inodes, old_parent_id, &old_name);
        insert_child(&mut self.inodes, new_parent_id, new_name, child_id);

        Ok(())
    }

    // ---- hard links ---------------------------------------------------------

    /// Create a hard link: `new_path` will point to the same inode as
    /// `existing_path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the existing path doesn't point to a file, or the
    /// new path already exists.
    ///
    /// # Panics
    ///
    /// Panics if internal inode invariants are violated (should never happen).
    pub fn hard_link(&mut self, existing_path: &str, new_path: &str) -> FsResult<()> {
        let existing_id = self.resolve_path(existing_path)?;
        let inode = self.inodes.get(&existing_id).ok_or(FsError::NotFound)?;
        if inode.is_dir() {
            return Err(FsError::IsADirectory);
        }

        let (parent_path, name) = Self::parent_and_name(new_path)?;
        let parent_id = self.resolve_path(&parent_path)?;

        let parent = self.inodes.get(&parent_id).ok_or(FsError::NotFound)?;
        match &parent.kind {
            InodeKind::Directory { children } => {
                if children.contains_key(&name) {
                    return Err(FsError::AlreadyExists);
                }
            }
            _ => return Err(FsError::NotADirectory),
        }

        // Add reference
        insert_child(&mut self.inodes, parent_id, name, existing_id);

        // Increment link count
        let inode_mut = self
            .inodes
            .get_mut(&existing_id)
            .expect("existing inode must exist");
        inode_mut.link_count += 1;

        Ok(())
    }

    // ---- truncate -----------------------------------------------------------

    /// Truncate a file to the given size.
    ///
    /// # Errors
    ///
    /// Returns an error if the path doesn't point to a writable file.
    #[allow(clippy::cast_possible_truncation)]
    pub fn truncate(&mut self, path: &str, size: u64) -> FsResult<()> {
        let id = self.resolve_path(path)?;
        let inode = self.inodes.get_mut(&id).ok_or(FsError::NotFound)?;
        if !inode.permissions.write {
            return Err(FsError::PermissionDenied);
        }
        match &mut inode.kind {
            InodeKind::File { data } => {
                data.resize(size as usize, 0);
                inode.size = size;
                Ok(())
            }
            _ => Err(FsError::IsADirectory),
        }
    }

    // ---- mkdir_p (recursive) ------------------------------------------------

    /// Recursively create directories (like `mkdir -p`).
    ///
    /// # Errors
    ///
    /// Returns an error if any non-directory component exists in the path.
    pub fn mkdir_p(&mut self, path: &str) -> FsResult<InodeId> {
        let components = Self::split_path(path)?;
        let mut current_path = String::new();
        let mut last_id = self.root_inode;

        for comp in components {
            current_path.push('/');
            current_path.push_str(comp);

            match self.resolve_path(&current_path) {
                Ok(id) => {
                    let inode = self.inodes.get(&id).ok_or(FsError::NotFound)?;
                    if !inode.is_dir() {
                        return Err(FsError::NotADirectory);
                    }
                    last_id = id;
                }
                Err(FsError::NotFound) => {
                    last_id = self.mkdir(&current_path)?;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(last_id)
    }

    /// Check if a path exists.
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.resolve_path(path).is_ok()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- basic construction ------------------------------------------------

    #[test]
    fn test_new_fs_has_root() {
        let fs = VirtualFs::new();
        assert!(fs.stat("/").is_ok());
        assert!(fs.stat("/").unwrap().is_dir());
    }

    #[test]
    fn test_default_is_new() {
        let fs = VirtualFs::default();
        assert!(fs.stat("/").is_ok());
    }

    #[test]
    fn test_root_inode_count() {
        let fs = VirtualFs::new();
        assert_eq!(fs.inode_count(), 1);
    }

    // ---- permissions -------------------------------------------------------

    #[test]
    fn test_permissions_all() {
        let p = Permissions::all();
        assert!(p.read && p.write && p.execute);
    }

    #[test]
    fn test_permissions_none() {
        let p = Permissions::none();
        assert!(!p.read && !p.write && !p.execute);
    }

    #[test]
    fn test_permissions_read_only() {
        let p = Permissions::read_only();
        assert!(p.read);
        assert!(!p.write);
        assert!(!p.execute);
    }

    #[test]
    fn test_permissions_read_write() {
        let p = Permissions::read_write();
        assert!(p.read && p.write && !p.execute);
    }

    #[test]
    fn test_permissions_octal_roundtrip() {
        for v in 0..=7 {
            let p = Permissions::from_octal(v);
            assert_eq!(p.as_octal(), v);
        }
    }

    #[test]
    fn test_permissions_display() {
        assert_eq!(format!("{}", Permissions::all()), "rwx");
        assert_eq!(format!("{}", Permissions::none()), "---");
        assert_eq!(format!("{}", Permissions::read_only()), "r--");
        assert_eq!(format!("{}", Permissions::from_octal(5)), "r-x");
    }

    // ---- mkdir -------------------------------------------------------------

    #[test]
    fn test_mkdir_basic() {
        let mut fs = VirtualFs::new();
        assert!(fs.mkdir("/home").is_ok());
        assert!(fs.stat("/home").unwrap().is_dir());
    }

    #[test]
    fn test_mkdir_nested() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/a").unwrap();
        fs.mkdir("/a/b").unwrap();
        assert!(fs.stat("/a/b").unwrap().is_dir());
    }

    #[test]
    fn test_mkdir_already_exists() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/dup").unwrap();
        assert_eq!(fs.mkdir("/dup"), Err(FsError::AlreadyExists));
    }

    #[test]
    fn test_mkdir_parent_not_found() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.mkdir("/no/such/parent"), Err(FsError::NotFound));
    }

    #[test]
    fn test_mkdir_with_perms() {
        let mut fs = VirtualFs::new();
        fs.mkdir_with_perms("/ro", Permissions::read_only())
            .unwrap();
        let inode = fs.stat("/ro").unwrap();
        assert!(!inode.permissions.write);
    }

    #[test]
    fn test_mkdir_in_read_only_parent() {
        let mut fs = VirtualFs::new();
        fs.mkdir_with_perms("/ro", Permissions::read_only())
            .unwrap();
        assert_eq!(fs.mkdir("/ro/child"), Err(FsError::PermissionDenied));
    }

    // ---- mkdir_p -----------------------------------------------------------

    #[test]
    fn test_mkdir_p_creates_chain() {
        let mut fs = VirtualFs::new();
        fs.mkdir_p("/a/b/c/d").unwrap();
        assert!(fs.stat("/a").unwrap().is_dir());
        assert!(fs.stat("/a/b").unwrap().is_dir());
        assert!(fs.stat("/a/b/c").unwrap().is_dir());
        assert!(fs.stat("/a/b/c/d").unwrap().is_dir());
    }

    #[test]
    fn test_mkdir_p_existing_ok() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/a").unwrap();
        assert!(fs.mkdir_p("/a/b").is_ok());
    }

    // ---- create_file -------------------------------------------------------

    #[test]
    fn test_create_file() {
        let mut fs = VirtualFs::new();
        fs.create_file("/hello.txt").unwrap();
        assert!(fs.stat("/hello.txt").unwrap().is_file());
    }

    #[test]
    fn test_create_file_duplicate() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        assert_eq!(fs.create_file("/f"), Err(FsError::AlreadyExists));
    }

    #[test]
    fn test_create_file_in_subdir() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/etc").unwrap();
        fs.create_file("/etc/config").unwrap();
        assert!(fs.stat("/etc/config").unwrap().is_file());
    }

    #[test]
    fn test_create_file_no_parent() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.create_file("/no/f"), Err(FsError::NotFound));
    }

    #[test]
    fn test_create_file_with_perms() {
        let mut fs = VirtualFs::new();
        fs.create_file_with_perms("/x", Permissions::from_octal(5))
            .unwrap();
        let inode = fs.stat("/x").unwrap();
        assert!(inode.permissions.read && inode.permissions.execute);
        assert!(!inode.permissions.write);
    }

    // ---- read / write file -------------------------------------------------

    #[test]
    fn test_write_and_read_file() {
        let mut fs = VirtualFs::new();
        fs.create_file("/data").unwrap();
        fs.write_file("/data", b"hello").unwrap();
        assert_eq!(fs.read_file("/data").unwrap(), b"hello");
    }

    #[test]
    fn test_write_overwrites() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"aaa").unwrap();
        fs.write_file("/f", b"bb").unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"bb");
    }

    #[test]
    fn test_read_empty_file() {
        let mut fs = VirtualFs::new();
        fs.create_file("/e").unwrap();
        assert!(fs.read_file("/e").unwrap().is_empty());
    }

    #[test]
    fn test_write_to_readonly() {
        let mut fs = VirtualFs::new();
        fs.create_file_with_perms("/ro", Permissions::read_only())
            .unwrap();
        assert_eq!(fs.write_file("/ro", b"x"), Err(FsError::PermissionDenied));
    }

    #[test]
    fn test_read_from_no_read() {
        let mut fs = VirtualFs::new();
        fs.create_file_with_perms("/wo", Permissions::new(false, true, false))
            .unwrap();
        assert_eq!(fs.read_file("/wo"), Err(FsError::PermissionDenied));
    }

    #[test]
    fn test_write_to_dir_fails() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        assert_eq!(fs.write_file("/d", b"x"), Err(FsError::IsADirectory));
    }

    #[test]
    fn test_read_from_dir_fails() {
        let fs = VirtualFs::new();
        assert_eq!(fs.read_file("/"), Err(FsError::IsADirectory));
    }

    // ---- list_dir ----------------------------------------------------------

    #[test]
    fn test_list_dir_empty_root() {
        let fs = VirtualFs::new();
        assert!(fs.list_dir("/").unwrap().is_empty());
    }

    #[test]
    fn test_list_dir_sorted() {
        let mut fs = VirtualFs::new();
        fs.create_file("/z").unwrap();
        fs.create_file("/a").unwrap();
        fs.mkdir("/m").unwrap();
        let names = fs.list_dir("/").unwrap();
        assert_eq!(names, vec!["a", "m", "z"]);
    }

    #[test]
    fn test_list_dir_not_a_dir() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        assert_eq!(fs.list_dir("/f"), Err(FsError::NotADirectory));
    }

    #[test]
    fn test_list_dir_no_read_perm() {
        let mut fs = VirtualFs::new();
        fs.mkdir_with_perms("/secret", Permissions::new(false, true, false))
            .unwrap();
        assert_eq!(fs.list_dir("/secret"), Err(FsError::PermissionDenied));
    }

    // ---- unlink ------------------------------------------------------------

    #[test]
    fn test_unlink_file() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.unlink("/f").unwrap();
        assert!(!fs.exists("/f"));
    }

    #[test]
    fn test_unlink_not_found() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.unlink("/nope"), Err(FsError::NotFound));
    }

    #[test]
    fn test_unlink_dir_fails() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        assert_eq!(fs.unlink("/d"), Err(FsError::IsADirectory));
    }

    #[test]
    fn test_unlink_symlink() {
        let mut fs = VirtualFs::new();
        fs.create_file("/target").unwrap();
        fs.create_symlink("/link", "/target").unwrap();
        fs.unlink("/link").unwrap();
        assert!(!fs.exists("/link"));
        assert!(fs.exists("/target"));
    }

    // ---- rmdir -------------------------------------------------------------

    #[test]
    fn test_rmdir_empty() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        fs.rmdir("/d").unwrap();
        assert!(!fs.exists("/d"));
    }

    #[test]
    fn test_rmdir_not_empty() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        fs.create_file("/d/f").unwrap();
        assert_eq!(fs.rmdir("/d"), Err(FsError::NotEmpty));
    }

    #[test]
    fn test_rmdir_not_a_dir() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        assert_eq!(fs.rmdir("/f"), Err(FsError::NotADirectory));
    }

    // ---- symlinks ----------------------------------------------------------

    #[test]
    fn test_create_symlink() {
        let mut fs = VirtualFs::new();
        fs.create_file("/real").unwrap();
        fs.create_symlink("/link", "/real").unwrap();
        let inode = fs.lstat("/link").unwrap();
        assert!(inode.is_symlink());
    }

    #[test]
    fn test_symlink_follow_read() {
        let mut fs = VirtualFs::new();
        fs.create_file("/real").unwrap();
        fs.write_file("/real", b"data").unwrap();
        fs.create_symlink("/link", "/real").unwrap();
        assert_eq!(fs.read_file("/link").unwrap(), b"data");
    }

    #[test]
    fn test_symlink_follow_write() {
        let mut fs = VirtualFs::new();
        fs.create_file("/real").unwrap();
        fs.create_symlink("/link", "/real").unwrap();
        fs.write_file("/link", b"via_link").unwrap();
        assert_eq!(fs.read_file("/real").unwrap(), b"via_link");
    }

    #[test]
    fn test_read_link() {
        let mut fs = VirtualFs::new();
        fs.create_file("/target").unwrap();
        fs.create_symlink("/l", "/target").unwrap();
        assert_eq!(fs.read_link("/l").unwrap(), "/target");
    }

    #[test]
    fn test_read_link_not_symlink() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        assert_eq!(fs.read_link("/f"), Err(FsError::NotASymlink));
    }

    #[test]
    fn test_symlink_to_dir() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/real_dir").unwrap();
        fs.create_file("/real_dir/inside").unwrap();
        fs.create_symlink("/link_dir", "/real_dir").unwrap();
        assert_eq!(
            fs.list_dir("/link_dir").unwrap(),
            vec!["inside".to_string()]
        );
    }

    #[test]
    fn test_symlink_chain() {
        let mut fs = VirtualFs::new();
        fs.create_file("/a").unwrap();
        fs.write_file("/a", b"chain").unwrap();
        fs.create_symlink("/b", "/a").unwrap();
        fs.create_symlink("/c", "/b").unwrap();
        assert_eq!(fs.read_file("/c").unwrap(), b"chain");
    }

    #[test]
    fn test_symlink_loop_detected() {
        let mut fs = VirtualFs::new();
        fs.create_symlink("/x", "/y").unwrap();
        fs.create_symlink("/y", "/x").unwrap();
        assert_eq!(fs.resolve_path("/x"), Err(FsError::SymlinkLoop));
    }

    #[test]
    fn test_dangling_symlink() {
        let mut fs = VirtualFs::new();
        fs.create_symlink("/dangling", "/nonexistent").unwrap();
        assert_eq!(fs.read_file("/dangling"), Err(FsError::NotFound));
    }

    // ---- lstat vs stat -----------------------------------------------------

    #[test]
    fn test_stat_follows_symlink() {
        let mut fs = VirtualFs::new();
        fs.create_file("/real").unwrap();
        fs.create_symlink("/link", "/real").unwrap();
        assert!(fs.stat("/link").unwrap().is_file());
    }

    #[test]
    fn test_lstat_does_not_follow() {
        let mut fs = VirtualFs::new();
        fs.create_file("/real").unwrap();
        fs.create_symlink("/link", "/real").unwrap();
        assert!(fs.lstat("/link").unwrap().is_symlink());
    }

    // ---- file descriptors --------------------------------------------------

    #[test]
    fn test_open_and_close() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Read).unwrap();
        assert!(fs.close(fd).is_ok());
    }

    #[test]
    fn test_open_nonexistent() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.open("/nope", OpenMode::Read), Err(FsError::NotFound));
    }

    #[test]
    fn test_open_dir_fails() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.open("/", OpenMode::Read), Err(FsError::IsADirectory));
    }

    #[test]
    fn test_close_invalid_fd() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.close(999), Err(FsError::InvalidFileDescriptor));
    }

    #[test]
    fn test_fd_read() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"hello").unwrap();
        let fd = fs.open("/f", OpenMode::Read).unwrap();
        let data = fs.read(fd, 3).unwrap();
        assert_eq!(data, b"hel");
        let data2 = fs.read(fd, 10).unwrap();
        assert_eq!(data2, b"lo");
    }

    #[test]
    fn test_fd_read_eof() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Read).unwrap();
        assert!(fs.read(fd, 10).unwrap().is_empty());
    }

    #[test]
    fn test_fd_write() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Write).unwrap();
        fs.write(fd, b"abc").unwrap();
        fs.close(fd).unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"abc");
    }

    #[test]
    fn test_fd_write_extends() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Write).unwrap();
        fs.write(fd, b"abc").unwrap();
        fs.write(fd, b"de").unwrap();
        fs.close(fd).unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"abcde");
    }

    #[test]
    fn test_fd_readwrite() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::ReadWrite).unwrap();
        fs.write(fd, b"hello").unwrap();
        fs.seek(fd, 0).unwrap();
        let data = fs.read(fd, 5).unwrap();
        assert_eq!(data, b"hello");
    }

    #[test]
    fn test_fd_append() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"start").unwrap();
        let fd = fs.open("/f", OpenMode::Append).unwrap();
        fs.write(fd, b"_end").unwrap();
        fs.close(fd).unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"start_end");
    }

    #[test]
    fn test_fd_read_on_write_only() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Write).unwrap();
        assert_eq!(fs.read(fd, 1), Err(FsError::WriteOnly));
    }

    #[test]
    fn test_fd_write_on_read_only() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Read).unwrap();
        assert_eq!(fs.write(fd, b"x"), Err(FsError::ReadOnly));
    }

    #[test]
    fn test_seek_and_tell() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"0123456789").unwrap();
        let fd = fs.open("/f", OpenMode::Read).unwrap();
        fs.seek(fd, 5).unwrap();
        assert_eq!(fs.tell(fd).unwrap(), 5);
        let data = fs.read(fd, 3).unwrap();
        assert_eq!(data, b"567");
    }

    #[test]
    fn test_tell_invalid_fd() {
        let fs = VirtualFs::new();
        assert_eq!(fs.tell(0), Err(FsError::InvalidFileDescriptor));
    }

    #[test]
    fn test_read_invalid_fd() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.read(99, 1), Err(FsError::InvalidFileDescriptor));
    }

    #[test]
    fn test_write_invalid_fd() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.write(99, b"x"), Err(FsError::InvalidFileDescriptor));
    }

    #[test]
    fn test_seek_invalid_fd() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.seek(99, 0), Err(FsError::InvalidFileDescriptor));
    }

    #[test]
    fn test_open_no_read_perm() {
        let mut fs = VirtualFs::new();
        fs.create_file_with_perms("/f", Permissions::new(false, true, false))
            .unwrap();
        assert_eq!(
            fs.open("/f", OpenMode::Read),
            Err(FsError::PermissionDenied)
        );
    }

    #[test]
    fn test_open_no_write_perm() {
        let mut fs = VirtualFs::new();
        fs.create_file_with_perms("/f", Permissions::read_only())
            .unwrap();
        assert_eq!(
            fs.open("/f", OpenMode::Write),
            Err(FsError::PermissionDenied)
        );
    }

    #[test]
    fn test_open_rw_needs_both_perms() {
        let mut fs = VirtualFs::new();
        fs.create_file_with_perms("/f", Permissions::read_only())
            .unwrap();
        assert_eq!(
            fs.open("/f", OpenMode::ReadWrite),
            Err(FsError::PermissionDenied)
        );
    }

    // ---- chmod -------------------------------------------------------------

    #[test]
    fn test_chmod() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.chmod("/f", Permissions::none()).unwrap();
        let inode = fs.stat("/f").unwrap();
        assert_eq!(inode.permissions, Permissions::none());
    }

    #[test]
    fn test_chmod_not_found() {
        let mut fs = VirtualFs::new();
        assert_eq!(
            fs.chmod("/nope", Permissions::all()),
            Err(FsError::NotFound)
        );
    }

    // ---- rename ------------------------------------------------------------

    #[test]
    fn test_rename_file() {
        let mut fs = VirtualFs::new();
        fs.create_file("/old").unwrap();
        fs.write_file("/old", b"data").unwrap();
        fs.rename("/old", "/new").unwrap();
        assert!(!fs.exists("/old"));
        assert_eq!(fs.read_file("/new").unwrap(), b"data");
    }

    #[test]
    fn test_rename_dir() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d1").unwrap();
        fs.create_file("/d1/f").unwrap();
        fs.rename("/d1", "/d2").unwrap();
        assert!(!fs.exists("/d1"));
        assert!(fs.stat("/d2").unwrap().is_dir());
    }

    #[test]
    fn test_rename_to_existing_fails() {
        let mut fs = VirtualFs::new();
        fs.create_file("/a").unwrap();
        fs.create_file("/b").unwrap();
        assert_eq!(fs.rename("/a", "/b"), Err(FsError::AlreadyExists));
    }

    #[test]
    fn test_rename_cross_dir() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d1").unwrap();
        fs.mkdir("/d2").unwrap();
        fs.create_file("/d1/f").unwrap();
        fs.write_file("/d1/f", b"moved").unwrap();
        fs.rename("/d1/f", "/d2/f").unwrap();
        assert_eq!(fs.read_file("/d2/f").unwrap(), b"moved");
    }

    // ---- hard links --------------------------------------------------------

    #[test]
    fn test_hard_link_basic() {
        let mut fs = VirtualFs::new();
        fs.create_file("/original").unwrap();
        fs.write_file("/original", b"shared").unwrap();
        fs.hard_link("/original", "/link").unwrap();
        assert_eq!(fs.read_file("/link").unwrap(), b"shared");
    }

    #[test]
    fn test_hard_link_shared_data() {
        let mut fs = VirtualFs::new();
        fs.create_file("/a").unwrap();
        fs.hard_link("/a", "/b").unwrap();
        fs.write_file("/a", b"updated").unwrap();
        assert_eq!(fs.read_file("/b").unwrap(), b"updated");
    }

    #[test]
    fn test_hard_link_count() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let id = fs.resolve_path("/f").unwrap();
        assert_eq!(fs.get_inode(id).unwrap().link_count, 1);
        fs.hard_link("/f", "/f2").unwrap();
        assert_eq!(fs.get_inode(id).unwrap().link_count, 2);
    }

    #[test]
    fn test_hard_link_unlink_one() {
        let mut fs = VirtualFs::new();
        fs.create_file("/a").unwrap();
        fs.write_file("/a", b"still alive").unwrap();
        fs.hard_link("/a", "/b").unwrap();
        fs.unlink("/a").unwrap();
        assert!(!fs.exists("/a"));
        assert_eq!(fs.read_file("/b").unwrap(), b"still alive");
    }

    #[test]
    fn test_hard_link_to_dir_fails() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        assert_eq!(fs.hard_link("/d", "/link"), Err(FsError::IsADirectory));
    }

    // ---- truncate ----------------------------------------------------------

    #[test]
    fn test_truncate_smaller() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"hello world").unwrap();
        fs.truncate("/f", 5).unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"hello");
    }

    #[test]
    fn test_truncate_larger() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"hi").unwrap();
        fs.truncate("/f", 5).unwrap();
        let data = fs.read_file("/f").unwrap();
        assert_eq!(data.len(), 5);
        assert_eq!(&data[..2], b"hi");
        assert_eq!(&data[2..], &[0, 0, 0]);
    }

    #[test]
    fn test_truncate_to_zero() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"stuff").unwrap();
        fs.truncate("/f", 0).unwrap();
        assert!(fs.read_file("/f").unwrap().is_empty());
    }

    #[test]
    fn test_truncate_readonly() {
        let mut fs = VirtualFs::new();
        fs.create_file_with_perms("/f", Permissions::read_only())
            .unwrap();
        assert_eq!(fs.truncate("/f", 0), Err(FsError::PermissionDenied));
    }

    #[test]
    fn test_truncate_dir_fails() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        assert_eq!(fs.truncate("/d", 0), Err(FsError::IsADirectory));
    }

    // ---- mounting ----------------------------------------------------------

    #[test]
    fn test_mount_basic() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/mnt").unwrap();
        fs.mount("/mnt", "usb").unwrap();
        assert_eq!(fs.list_mounts().len(), 1);
        assert_eq!(fs.list_mounts()[0].label, "usb");
    }

    #[test]
    fn test_mount_not_empty() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/mnt").unwrap();
        fs.create_file("/mnt/x").unwrap();
        assert_eq!(fs.mount("/mnt", "x"), Err(FsError::MountPointNotEmpty));
    }

    #[test]
    fn test_unmount() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/mnt").unwrap();
        fs.mount("/mnt", "usb").unwrap();
        fs.unmount("/mnt").unwrap();
        assert!(fs.list_mounts().is_empty());
    }

    #[test]
    fn test_unmount_not_found() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.unmount("/nope"), Err(FsError::MountPointNotFound));
    }

    // ---- buffered I/O ------------------------------------------------------

    #[test]
    fn test_buf_writer_basic() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Write).unwrap();
        let mut bw = BufWriter::new(fd, 8);
        bw.write(&mut fs, b"hello").unwrap();
        assert_eq!(bw.buffered_len(), 5);
        bw.flush(&mut fs).unwrap();
        assert_eq!(bw.buffered_len(), 0);
        fs.close(fd).unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"hello");
    }

    #[test]
    fn test_buf_writer_auto_flush() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Write).unwrap();
        let mut bw = BufWriter::new(fd, 4);
        bw.write(&mut fs, b"abcde").unwrap();
        assert!(bw.buffered_len() <= 4);
        bw.flush(&mut fs).unwrap();
        fs.close(fd).unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"abcde");
    }

    #[test]
    fn test_buf_reader_basic() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"hello world!").unwrap();
        let fd = fs.open("/f", OpenMode::Read).unwrap();
        let mut br = BufReader::new(fd, 4);
        let data = br.read(&mut fs, 5).unwrap();
        assert_eq!(data, b"hello");
        let data2 = br.read(&mut fs, 20).unwrap();
        assert_eq!(data2, b" world!");
    }

    #[test]
    fn test_buf_reader_eof() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Read).unwrap();
        let mut br = BufReader::new(fd, 4);
        let data = br.read(&mut fs, 10).unwrap();
        assert!(data.is_empty());
    }

    // ---- path edge cases ---------------------------------------------------

    #[test]
    fn test_invalid_relative_path() {
        let fs = VirtualFs::new();
        assert_eq!(fs.resolve_path("relative"), Err(FsError::InvalidPath));
    }

    #[test]
    fn test_root_path() {
        let fs = VirtualFs::new();
        assert!(fs.resolve_path("/").is_ok());
    }

    #[test]
    fn test_dot_in_path() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/a").unwrap();
        let id1 = fs.resolve_path("/a").unwrap();
        let id2 = fs.resolve_path("/a/.").unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_dotdot_at_root() {
        let fs = VirtualFs::new();
        let id = fs.resolve_path("/..").unwrap();
        assert_eq!(id, fs.root_inode);
    }

    #[test]
    fn test_double_slash() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/a").unwrap();
        assert!(fs.resolve_path("//a").is_ok());
    }

    // ---- exists ------------------------------------------------------------

    #[test]
    fn test_exists_true() {
        let fs = VirtualFs::new();
        assert!(fs.exists("/"));
    }

    #[test]
    fn test_exists_false() {
        let fs = VirtualFs::new();
        assert!(!fs.exists("/nope"));
    }

    // ---- inode properties --------------------------------------------------

    #[test]
    fn test_inode_is_dir() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        let inode = fs.stat("/d").unwrap();
        assert!(inode.is_dir());
        assert!(!inode.is_file());
        assert!(!inode.is_symlink());
    }

    #[test]
    fn test_inode_is_file() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let inode = fs.stat("/f").unwrap();
        assert!(inode.is_file());
        assert!(!inode.is_dir());
    }

    #[test]
    fn test_inode_is_symlink() {
        let mut fs = VirtualFs::new();
        fs.create_file("/t").unwrap();
        fs.create_symlink("/l", "/t").unwrap();
        let inode = fs.lstat("/l").unwrap();
        assert!(inode.is_symlink());
    }

    #[test]
    fn test_file_size_updated() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"12345").unwrap();
        assert_eq!(fs.stat("/f").unwrap().size, 5);
    }

    #[test]
    fn test_symlink_size_is_target_length() {
        let mut fs = VirtualFs::new();
        fs.create_symlink("/l", "/some/target").unwrap();
        let inode = fs.lstat("/l").unwrap();
        assert_eq!(inode.size, "/some/target".len() as u64);
    }

    // ---- multiple fd ops ---------------------------------------------------

    #[test]
    fn test_multiple_fds_independent() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        fs.write_file("/f", b"abcdef").unwrap();
        let fd1 = fs.open("/f", OpenMode::Read).unwrap();
        let fd2 = fs.open("/f", OpenMode::Read).unwrap();
        let _ = fs.read(fd1, 3).unwrap();
        assert_eq!(fs.tell(fd1).unwrap(), 3);
        assert_eq!(fs.tell(fd2).unwrap(), 0);
    }

    // ---- error display -----------------------------------------------------

    #[test]
    fn test_error_display() {
        assert_eq!(format!("{}", FsError::NotFound), "not found");
        assert_eq!(format!("{}", FsError::AlreadyExists), "already exists");
        assert_eq!(format!("{}", FsError::InvalidPath), "invalid path");
        assert_eq!(
            format!("{}", FsError::SymlinkLoop),
            "too many symlink levels"
        );
    }

    // ---- inode_count -------------------------------------------------------

    #[test]
    fn test_inode_count_grows() {
        let mut fs = VirtualFs::new();
        assert_eq!(fs.inode_count(), 1);
        fs.mkdir("/a").unwrap();
        assert_eq!(fs.inode_count(), 2);
        fs.create_file("/f").unwrap();
        assert_eq!(fs.inode_count(), 3);
    }

    #[test]
    fn test_inode_count_shrinks_on_remove() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        assert_eq!(fs.inode_count(), 2);
        fs.unlink("/f").unwrap();
        assert_eq!(fs.inode_count(), 1);
    }

    // ---- fd on symlink target ----------------------------------------------

    #[test]
    fn test_open_via_symlink() {
        let mut fs = VirtualFs::new();
        fs.create_file("/real").unwrap();
        fs.write_file("/real", b"content").unwrap();
        fs.create_symlink("/link", "/real").unwrap();
        let fd = fs.open("/link", OpenMode::Read).unwrap();
        let data = fs.read(fd, 100).unwrap();
        assert_eq!(data, b"content");
    }

    // ---- write through fd then read whole file -----------------------------

    #[test]
    fn test_fd_write_then_direct_read() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Write).unwrap();
        fs.write(fd, b"one").unwrap();
        fs.write(fd, b"two").unwrap();
        fs.close(fd).unwrap();
        assert_eq!(fs.read_file("/f").unwrap(), b"onetwo");
    }

    // ---- get_inode ---------------------------------------------------------

    #[test]
    fn test_get_inode_valid() {
        let fs = VirtualFs::new();
        assert!(fs.get_inode(1).is_some());
    }

    #[test]
    fn test_get_inode_invalid() {
        let fs = VirtualFs::new();
        assert!(fs.get_inode(999).is_none());
    }

    // ---- parent_and_name edge cases ----------------------------------------

    #[test]
    fn test_parent_and_name_root_child() {
        let (p, n) = VirtualFs::parent_and_name("/foo").unwrap();
        assert_eq!(p, "/");
        assert_eq!(n, "foo");
    }

    #[test]
    fn test_parent_and_name_nested() {
        let (p, n) = VirtualFs::parent_and_name("/a/b/c").unwrap();
        assert_eq!(p, "/a/b");
        assert_eq!(n, "c");
    }

    #[test]
    fn test_parent_and_name_root_fails() {
        assert_eq!(VirtualFs::parent_and_name("/"), Err(FsError::InvalidPath));
    }

    // ---- big directory listing ----------------------------------------------

    #[test]
    fn test_many_files_in_dir() {
        let mut fs = VirtualFs::new();
        for i in 0..50 {
            fs.create_file(&format!("/file_{i}")).unwrap();
        }
        assert_eq!(fs.list_dir("/").unwrap().len(), 50);
    }

    // ---- create in read-only parent ----------------------------------------

    #[test]
    fn test_create_file_in_readonly_parent() {
        let mut fs = VirtualFs::new();
        fs.mkdir_with_perms("/ro", Permissions::read_only())
            .unwrap();
        assert_eq!(fs.create_file("/ro/nope"), Err(FsError::PermissionDenied));
    }

    // ---- unlink in read-only parent ----------------------------------------

    #[test]
    fn test_unlink_in_readonly_parent() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        fs.create_file("/d/f").unwrap();
        fs.chmod("/d", Permissions::read_only()).unwrap();
        assert_eq!(fs.unlink("/d/f"), Err(FsError::PermissionDenied));
    }

    // ---- rmdir in read-only parent -----------------------------------------

    #[test]
    fn test_rmdir_in_readonly_parent() {
        let mut fs = VirtualFs::new();
        fs.mkdir("/d").unwrap();
        fs.mkdir("/d/sub").unwrap();
        fs.chmod("/d", Permissions::read_only()).unwrap();
        assert_eq!(fs.rmdir("/d/sub"), Err(FsError::PermissionDenied));
    }

    // ---- mount on file fails -----------------------------------------------

    #[test]
    fn test_mount_on_file_fails() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        assert_eq!(fs.mount("/f", "x"), Err(FsError::NotADirectory));
    }

    // ---- open append no read -----------------------------------------------

    #[test]
    fn test_open_append_read_fails() {
        let mut fs = VirtualFs::new();
        fs.create_file("/f").unwrap();
        let fd = fs.open("/f", OpenMode::Append).unwrap();
        assert_eq!(fs.read(fd, 1), Err(FsError::WriteOnly));
    }
}
