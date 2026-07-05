//! ALICE-FileSystem: Pure Rust virtual filesystem.
//!
//! Provides inodes, directory tree, file permissions (rwx), symlinks,
//! mounting, path resolution, file descriptors, and buffered I/O abstraction.

#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::wildcard_imports,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::similar_names,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else
)]

pub mod buf_io;
pub mod errors;
pub mod fd;
pub mod inode;
pub mod mount;
pub mod permissions;
pub mod prelude;
pub mod virtual_fs;

#[cfg(test)]
mod integration_tests;

// Backward-compat re-exports.
pub use crate::buf_io::*;
pub use crate::errors::*;
pub use crate::fd::*;
pub use crate::inode::*;
pub use crate::mount::*;
pub use crate::permissions::*;
pub use crate::virtual_fs::*;
