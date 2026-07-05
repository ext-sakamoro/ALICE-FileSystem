//! Convenience re-export (= `use alice_filesystem::prelude::*;`).

pub use crate::buf_io::{BufReader, BufWriter};
pub use crate::errors::{FsError, FsResult};
pub use crate::fd::{FileDescriptor, OpenMode};
pub use crate::inode::{Inode, InodeKind};
pub use crate::mount::MountEntry;
pub use crate::permissions::Permissions;
pub use crate::virtual_fs::VirtualFs;
