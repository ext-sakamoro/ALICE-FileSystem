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

`AGPL-3.0 OR LicenseRef-Commercial` — dual-licensed. Pick either.

| Option | Terms | Use it when |
|--------|-------|-------------|
| **AGPL-3.0** | [LICENSE-AGPL](LICENSE-AGPL) — free, no reporting obligation | Your project is itself AGPL-compatible open source, or you are only using it internally |
| **Commercial License** | [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) — paid, removes the copyleft | Closed-source product, proprietary SaaS, edge / firmware distribution, plugin redistribution, or a platform NDA that forbids source disclosure |

AGPL is a strong copyleft: a product, firmware image, or service that links
`alice-filesystem` and is distributed or served to users must be released under the AGPL
as well. That is intentional for the open ecosystem, and the Commercial
License exists for the cases where it is not something you are able to do.

Commercial licence enquiries: <contact@extoria.co.jp>
