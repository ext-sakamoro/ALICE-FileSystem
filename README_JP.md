[English](README.md) | **日本語**

# ALICE-FileSystem

ALICEエコシステムの仮想ファイルシステムモジュール。iノード、ディレクトリツリー、Unix風パーミッション、シンボリックリンク、マウント、パス解決、ファイルディスクリプタ、バッファードI/Oを純Rustで実装。

## 概要

| 項目 | 値 |
|------|-----|
| **クレート名** | `alice-filesystem` |
| **バージョン** | 1.0.0 |
| **ライセンス** | AGPL-3.0 |
| **エディション** | 2021 |

## 機能

- **iノードベースストレージ** — 各ファイル/ディレクトリはメタデータ付きの一意なiノードで管理
- **ディレクトリツリー** — 作成・削除・一覧操作を持つ階層的名前空間
- **パーミッション** — Unix風 rwx パーミッションモデル（単一ユーザー簡略版）
- **シンボリックリンク** — シンボリックリンクの作成・解決（ループ検出付き）
- **マウント** — 任意のディレクトリパスへのサブファイルシステムのマウント/アンマウント
- **パス解決** — シンボリックリンク追従付きの絶対/相対パス解決
- **ファイルディスクリプタ** — 整数FDによるopen/close/read/write
- **バッファードI/O** — 効率的なバイトレベルアクセスのための読み書きバッファ

## アーキテクチャ

```
alice-filesystem (lib.rs — 単一ファイルクレート)
├── FsError / FsResult          # エラー型
├── Permissions                  # rwx パーミッションモデル
├── Inode / InodeKind            # File/Dir/Symlink ノード
├── FileDescriptor / OpenMode    # FD抽象化
├── MountPoint                   # サブファイルシステムマウント
└── VirtualFs                    # トップレベルFSエンジン
```

## クイックスタート

```rust
use alice_filesystem::VirtualFs;

let mut fs = VirtualFs::new();
fs.mkdir("/home").unwrap();
fs.create("/home/hello.txt").unwrap();
let fd = fs.open("/home/hello.txt", OpenMode::Write).unwrap();
fs.write(fd, b"Hello, ALICE!").unwrap();
fs.close(fd).unwrap();
```

## ビルド

```bash
cargo build
cargo test
cargo clippy -- -W clippy::all
```

## ライセンス

AGPL-3.0 — 詳細は [LICENSE](LICENSE) を参照。
