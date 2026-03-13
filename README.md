**English** | [日本語](README_JP.md)

# ALICE-FileSystem

Virtual filesystem module for the ALICE ecosystem. Pure Rust implementation with inodes, directory tree, Unix-style permissions, symlinks, mounting, path resolution, file descriptors, and buffered I/O.

## Overview

| Item | Value |
|------|-------|
| **Crate** | `alice-filesystem` |
| **Version** | 1.0.0 |
| **License** | AGPL-3.0 |
| **Edition** | 2021 |

## Features

- **Inode-based Storage** — Each file/directory is backed by a unique inode with metadata
- **Directory Tree** — Hierarchical namespace with create, remove, and list operations
- **Permissions** — Unix-style rwx permission model (single-user simplified)
- **Symlinks** — Symbolic link creation and resolution with loop detection
- **Mounting** — Mount/unmount sub-filesystems at arbitrary directory paths
- **Path Resolution** — Absolute and relative path resolution with symlink following
- **File Descriptors** — Open/close/read/write via integer file descriptors
- **Buffered I/O** — Read and write buffers for efficient byte-level access

## Architecture

```
alice-filesystem (lib.rs — single-file crate)
├── FsError / FsResult          # Error types
├── Permissions                  # rwx permission model
├── Inode / InodeKind            # File/Dir/Symlink nodes
├── FileDescriptor / OpenMode    # FD abstraction
├── MountPoint                   # Sub-filesystem mounting
└── VirtualFs                    # Top-level filesystem engine
```

## Quick Start

```rust
use alice_filesystem::VirtualFs;

let mut fs = VirtualFs::new();
fs.mkdir("/home").unwrap();
fs.create("/home/hello.txt").unwrap();
let fd = fs.open("/home/hello.txt", OpenMode::Write).unwrap();
fs.write(fd, b"Hello, ALICE!").unwrap();
fs.close(fd).unwrap();
```

## Build

```bash
cargo build
cargo test
cargo clippy -- -W clippy::all
```

## License

AGPL-3.0 -- see [LICENSE](LICENSE) for details.
