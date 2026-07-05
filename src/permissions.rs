//! `Permissions` — Unix-style rwx permissions.

use std::fmt;

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
