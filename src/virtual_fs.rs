//! `VirtualFs` — virtual filesystem engine.

use crate::errors::{FsError, FsResult};
use crate::fd::{FileDescriptor, OpenMode};
use crate::inode::{Inode, InodeId, InodeKind};
use crate::mount::MountEntry;
use crate::permissions::Permissions;
use std::collections::HashMap;

// Virtual Filesystem
// ---------------------------------------------------------------------------

/// The main virtual filesystem struct.
#[derive(Debug)]
pub struct VirtualFs {
    inodes: HashMap<InodeId, Inode>,
    next_inode: InodeId,
    pub(crate) root_inode: InodeId,
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
    pub fn parent_and_name(path: &str) -> FsResult<(String, String)> {
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
