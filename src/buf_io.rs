//! Buffered I/O (`BufWriter` / `BufReader`).

use crate::errors::FsResult;
use crate::virtual_fs::VirtualFs;

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
