use crate::model::{portable_relative, Entry, Manifest, Notice};
use sha2::{Digest, Sha256};
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        a.len() == b.len() && a.modified().ok() == b.modified().ok()
    }
}

fn hash_file(path: &Path) -> io::Result<(u64, String)> {
    let before = fs::symlink_metadata(path)?;
    if !before.file_type().is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    let mut file = File::open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || !same_file(&before, &opened) {
        return Err(io::Error::other("file changed before reading"));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        bytes = bytes
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("file too large"))?;
    }
    let after = file.metadata()?;
    if bytes != opened.len()
        || after.len() != opened.len()
        || after.modified().ok() != opened.modified().ok()
    {
        return Err(io::Error::other("file changed while reading"));
    }
    Ok((bytes, format!("{:x}", digest.finalize())))
}

fn walk(root: &Path, directory: &Path, manifest: &mut Manifest) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            manifest.errors.push(Notice {
                path: directory
                    .strip_prefix(root)
                    .unwrap_or(directory)
                    .to_string_lossy()
                    .into(),
                reason: error.to_string(),
            });
            return;
        }
    };
    for item in entries {
        let entry = match item {
            Ok(entry) => entry,
            Err(error) => {
                manifest.errors.push(Notice {
                    path: ".".into(),
                    reason: error.to_string(),
                });
                continue;
            }
        };
        let path = entry.path();
        let relative = path.strip_prefix(root).expect("entry under supplied root");
        let name = match relative.to_str() {
            Some(value) => value.replace(std::path::MAIN_SEPARATOR, "/"),
            None => {
                manifest.errors.push(Notice {
                    path: relative.to_string_lossy().into(),
                    reason: "non-UTF-8 path cannot be represented without loss".into(),
                });
                continue;
            }
        };
        if !portable_relative(&name) {
            manifest.errors.push(Notice {
                path: name,
                reason: "path is not portable".into(),
            });
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_symlink() => manifest.skipped.push(Notice {
                path: name,
                reason: "symbolic link not followed".into(),
            }),
            Ok(kind) if kind.is_dir() => walk(root, &path, manifest),
            Ok(kind) if kind.is_file() => match hash_file(&path) {
                Ok((bytes, sha256)) => manifest.files.push(Entry {
                    path: name,
                    bytes,
                    sha256,
                }),
                Err(error) => manifest.errors.push(Notice {
                    path: name,
                    reason: error.to_string(),
                }),
            },
            Ok(_) => manifest.skipped.push(Notice {
                path: name,
                reason: "special file not opened".into(),
            }),
            Err(error) => manifest.errors.push(Notice {
                path: name,
                reason: error.to_string(),
            }),
        }
    }
}

pub fn scan(root: &Path) -> Result<Manifest, String> {
    if fs::symlink_metadata(root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("root must be a real directory, not a symbolic link".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("root is not a directory".into());
    }
    let mut manifest = Manifest {
        format_version: 1,
        algorithm: "sha256".into(),
        generated_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs(),
        files: vec![],
        skipped: vec![],
        errors: vec![],
    };
    walk(&root, &root, &mut manifest);
    manifest.files.sort_by(|a, b| a.path.cmp(&b.path));
    manifest.skipped.sort_by(|a, b| a.path.cmp(&b.path));
    manifest.errors.sort_by(|a, b| a.path.cmp(&b.path));
    manifest.validate()?;
    Ok(manifest)
}
