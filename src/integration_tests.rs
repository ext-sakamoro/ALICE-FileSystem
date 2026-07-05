//! Integration tests spanning multiple modules.

#![allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::explicit_iter_loop,
    clippy::bool_to_int_with_if,
    clippy::approx_constant,
    clippy::cast_lossless,
    clippy::redundant_clone,
    clippy::format_collect,
    clippy::similar_names,
    clippy::needless_collect,
    clippy::iter_cloned_collect
)]

use crate::buf_io::*;
use crate::errors::*;
use crate::fd::*;
use crate::permissions::*;
use crate::virtual_fs::*;

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
