pub mod model;
pub mod scan;

use model::{Entry, Manifest};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct Difference {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub unchanged: usize,
}

impl Difference {
    pub fn is_clean(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

pub fn diff(before: &Manifest, after: &Manifest) -> Result<Difference, String> {
    before.validate()?;
    after.validate()?;
    if !before.errors.is_empty() || !after.errors.is_empty() {
        return Err("incomplete snapshot: resolve scan errors before comparing".into());
    }
    let left: BTreeMap<_, _> = before.files.iter().map(|f| (&f.path, f)).collect();
    let right: BTreeMap<_, _> = after.files.iter().map(|f| (&f.path, f)).collect();
    let paths: BTreeSet<_> = left.keys().chain(right.keys()).copied().collect();
    let mut result = Difference {
        added: vec![],
        removed: vec![],
        changed: vec![],
        unchanged: 0,
    };
    for path in paths {
        match (left.get(path), right.get(path)) {
            (None, Some(_)) => result.added.push(path.clone()),
            (Some(_), None) => result.removed.push(path.clone()),
            (Some(a), Some(b)) if a.bytes != b.bytes || a.sha256 != b.sha256 => {
                result.changed.push(path.clone())
            }
            (Some(_), Some(_)) => result.unchanged += 1,
            _ => unreachable!(),
        }
    }
    Ok(result)
}

#[derive(Debug, Serialize)]
pub struct DuplicateGroup {
    pub bytes_per_file: u64,
    pub sha256: String,
    pub paths: Vec<String>,
}

pub fn duplicates(manifest: &Manifest) -> Result<Vec<DuplicateGroup>, String> {
    manifest.validate()?;
    if !manifest.errors.is_empty() {
        return Err("incomplete snapshot: duplicate report would omit unreadable files".into());
    }
    let mut groups: BTreeMap<(u64, &str), Vec<String>> = BTreeMap::new();
    for Entry {
        path,
        bytes,
        sha256,
    } in &manifest.files
    {
        groups
            .entry((*bytes, sha256.as_str()))
            .or_default()
            .push(path.clone());
    }
    Ok(groups
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|((bytes, hash), mut paths)| {
            paths.sort();
            DuplicateGroup {
                bytes_per_file: bytes,
                sha256: hash.into(),
                paths,
            }
        })
        .collect())
}

pub fn load(path: &Path) -> Result<Manifest, String> {
    // A bound keeps a mistakenly supplied data dump from exhausting memory.
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("manifest exceeds 64 MiB".into());
    }
    let mut raw = Vec::new();
    file.take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    if raw.len() > 64 * 1024 * 1024 {
        return Err("manifest exceeds 64 MiB".into());
    }
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    manifest.validate()?;
    Ok(manifest)
}

pub fn write_new(path: &Path, manifest: &Manifest, root: &Path) -> Result<(), String> {
    manifest.validate()?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if parent.starts_with(root) {
        return Err("save the manifest outside the scanned directory".into());
    }
    let filename = path.file_name().ok_or("output needs a filename")?;
    let output = parent.join(filename);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = parent.join(format!(".file-ledger-{}-{stamp}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        serde_json::to_writer_pretty(&mut file, manifest).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        // Atomic publication without overwriting an existing destination.
        fs::hard_link(&temporary, &output).map_err(|e| e.to_string())?;
        Ok(())
    })();
    drop(file);
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;
    use tempfile::tempdir;

    #[test]
    fn known_hash_and_nested_unicode_path() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("данные")).unwrap();
        fs::write(dir.path().join("данные/hello.txt"), b"hello world").unwrap();
        let m = scan(dir.path()).unwrap();
        assert_eq!(m.files[0].path, "данные/hello.txt");
        assert_eq!(
            m.files[0].sha256,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert!(m.errors.is_empty());
    }

    #[test]
    fn same_size_different_content_is_not_duplicate() {
        let dir = tempdir().unwrap();
        for (name, value) in [("a", "one"), ("b", "two"), ("c", "one")] {
            fs::write(dir.path().join(name), value).unwrap();
        }
        let groups = duplicates(&scan(dir.path()).unwrap()).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths, vec!["a", "c"]);
    }

    #[test]
    fn detects_content_change_even_when_size_matches() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("same"), "old").unwrap();
        fs::write(dir.path().join("gone"), "gone").unwrap();
        let before = scan(dir.path()).unwrap();
        fs::write(dir.path().join("same"), "new").unwrap();
        fs::remove_file(dir.path().join("gone")).unwrap();
        fs::write(dir.path().join("added"), "new").unwrap();
        let d = diff(&before, &scan(dir.path()).unwrap()).unwrap();
        assert_eq!(d.changed, vec!["same"]);
        assert_eq!(d.removed, vec!["gone"]);
        assert_eq!(d.added, vec!["added"]);
        assert!(!d.is_clean());
    }

    #[test]
    fn rejects_traversal_duplicate_paths_and_malformed_hashes() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("ok"), "x").unwrap();
        let base = scan(dir.path()).unwrap();
        for path in ["../x", "/x", "a/../x", "a//x", "C:/x", "a\\x", "./x"] {
            let mut bad = base.clone();
            bad.files[0].path = path.into();
            assert!(bad.validate().is_err(), "{path}");
        }
        let mut bad = base.clone();
        bad.files.push(bad.files[0].clone());
        assert!(bad.validate().is_err());
        bad = base;
        bad.files[0].sha256 = "0".into();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn incomplete_manifest_cannot_produce_clean_verification() {
        let dir = tempdir().unwrap();
        let mut m = scan(dir.path()).unwrap();
        m.errors.push(model::Notice {
            path: "locked".into(),
            reason: "permission denied".into(),
        });
        assert!(diff(&m, &m).is_err());
        assert!(duplicates(&m).is_err());
    }

    #[test]
    fn publication_refuses_overwrite_and_self_inclusion() {
        let dir = tempdir().unwrap();
        let out = tempdir().unwrap();
        let m = scan(dir.path()).unwrap();
        let path = out.path().join("manifest.json");
        write_new(&path, &m, dir.path()).unwrap();
        let first = fs::read(&path).unwrap();
        assert!(write_new(&path, &m, dir.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), first);
        assert!(write_new(&dir.path().join("inside.json"), &m, dir.path()).is_err());
        assert_eq!(load(&path).unwrap().files, m.files);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_and_outside_target_are_not_followed() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret"), "not in inventory").unwrap();
        symlink(dir.path(), dir.path().join("loop")).unwrap();
        symlink(outside.path().join("secret"), dir.path().join("link")).unwrap();
        let m = scan(dir.path()).unwrap();
        assert!(m.files.is_empty());
        assert_eq!(m.skipped.len(), 2);
        assert!(scan(&dir.path().join("loop")).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_names_are_errors_not_lossy_duplicates() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(OsString::from_vec(vec![0xff])), "x").unwrap();
        let m = scan(dir.path()).unwrap();
        assert_eq!(m.errors.len(), 1);
        assert!(diff(&m, &m).is_err());
    }
}
