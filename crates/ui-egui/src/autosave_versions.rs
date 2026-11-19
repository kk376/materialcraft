//! Write numbered recovery copies and retain the newest copies for each document

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use craft_atomic::{NewFilePermissions, WriteOptions};
use materialcraft_engine::core::Package;
use materialcraft_engine::formats::project::ProjectFiles;
use sha2::{Digest, Sha256};

const EXTENSION: &str = ".mcraft";
const LOCK_FILE: &str = ".recovery.lock";

/// Saved documents share history by normalized path; untitled documents use workspace-local IDs
pub(crate) fn folder(root: &Path, path: Option<&str>, workspace: u64, document: u64) -> PathBuf {
    let key = match path.filter(|path| !path.contains("://")) {
        Some(path) => format!("file-{:x}", Sha256::digest(path.as_bytes())),
        None => format!("untitled-{workspace:016x}-{document}"),
    };
    root.join(key)
}

fn copies(folder: &Path) -> io::Result<impl Iterator<Item = io::Result<(u64, OsString)>>> {
    Ok(std::fs::read_dir(folder)?.filter_map(|entry| {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return Some(Err(error)),
        };

        let filename = entry.file_name();
        let number = filename.to_str()?.strip_suffix(EXTENSION)?.parse().ok()?;
        match entry.file_type() {
            Ok(kind) if kind.is_file() => Some(Ok((number, filename))),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        }
    }))
}

fn next_path(folder: &Path) -> io::Result<PathBuf> {
    let mut version = copies(folder)?.try_fold(0, |latest, copy| copy.map(|(number, _)| latest.max(number)))?;
    loop {
        version = version.checked_add(1).ok_or_else(|| io::Error::other("autosave versions exhausted"))?;
        let path = folder.join(format!("{version}{EXTENSION}"));

        // A directory or link can occupy a name without counting toward retention
        match path.symlink_metadata() {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(path),
            Err(error) => return Err(error),
            Ok(_) => continue,
        }
    }
}

fn has_recovery_footer(path: &Path) -> io::Result<bool> {
    // The recovery writer emits uncommented ZIP32 and ZIP64 end records
    const ZIP_FOOTER_BYTES: usize = 22;
    const ZIP64_FOOTER_BYTES: usize = 56;
    const ZIP64_LOCATOR_BYTES: usize = 20;
    let mut footer = [0; ZIP_FOOTER_BYTES];
    let mut file = File::open(path)?;
    if file.metadata()?.len() < ZIP_FOOTER_BYTES as u64 {
        return Ok(false);
    }

    let footer_offset = file.seek(SeekFrom::End(-(ZIP_FOOTER_BYTES as i64)))?;
    file.read_exact(&mut footer)?;
    if !footer.starts_with(b"PK\x05\x06") || !footer.ends_with(&[0, 0]) {
        return Ok(false);
    }

    let directory_size = u64::from(u32::from_le_bytes(footer[12..16].try_into().unwrap_or_default()));
    let directory_offset = u64::from(u32::from_le_bytes(footer[16..20].try_into().unwrap_or_default()));
    if directory_matches(&mut file, directory_offset, directory_size, footer_offset)? {
        return Ok(true);
    }

    let Some(zip64_offset) = footer_offset.checked_sub((ZIP64_FOOTER_BYTES + ZIP64_LOCATOR_BYTES) as u64) else {
        return Ok(false);
    };

    let mut zip64_footer = [0; ZIP64_FOOTER_BYTES];
    let mut locator = [0; ZIP64_LOCATOR_BYTES];
    file.seek(SeekFrom::Start(zip64_offset))?;
    file.read_exact(&mut zip64_footer)?;
    file.read_exact(&mut locator)?;
    if !zip64_footer.starts_with(b"PK\x06\x06") || !locator.starts_with(b"PK\x06\x07") || locator[8..16] != zip64_offset.to_le_bytes() {
        return Ok(false);
    }

    let directory_size = u64::from_le_bytes(zip64_footer[40..48].try_into().unwrap_or_default());
    let directory_offset = u64::from_le_bytes(zip64_footer[48..56].try_into().unwrap_or_default());
    directory_matches(&mut file, directory_offset, directory_size, zip64_offset)
}

fn directory_matches(file: &mut File, offset: u64, size: u64, end: u64) -> io::Result<bool> {
    // A nested ZIP in a torn copy can have a footer without pointing at the outer directory
    let mut signature = [0; 4];
    if size < signature.len() as u64 || offset.checked_add(size) != Some(end) {
        return Ok(false);
    }

    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut signature)?;
    Ok(&signature == b"PK\x01\x02")
}

fn prune(folder: &Path, keep: u32, mut remove: impl FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
    let keep = keep.clamp(1, crate::state::Prefs::MAX_AUTOSAVE_VERSIONS) as usize;
    let mut newest = BTreeSet::new();

    // Retain only the newest candidates before deleting anything
    for copy in copies(folder)? {
        let copy = copy?;
        if newest.len() == keep && newest.first().is_some_and(|oldest| &copy < oldest) {
            continue;
        }

        if has_recovery_footer(&folder.join(&copy.1))? {
            newest.insert(copy);
            if newest.len() > keep {
                newest.pop_first();
            }
        }
    }

    if newest.len() < keep {
        return Ok(());
    }

    let Some(oldest) = newest.into_iter().next() else {
        return Ok(());
    };

    // A second scan lets even an oversized folder be pruned with bounded storage
    let mut first_error = None;
    for copy in copies(folder)? {
        let result = copy.and_then(|copy| {
            if copy < oldest {
                let path = folder.join(copy.1);
                if has_recovery_footer(&path)? {
                    return remove(&path);
                }
            }

            Ok(())
        });
        if let Err(error) = result
            && error.kind() != ErrorKind::NotFound
        {
            first_error.get_or_insert(error);
        }
    }

    first_error.map_or(Ok(()), Err)
}

pub(crate) fn save(folder: &Path, package: &Package, keep: u32) -> io::Result<()> {
    let files = ProjectFiles::build(package, "autosave.mcraft", true).map_err(io::Error::other)?;
    save_with(folder, keep, |file| files.write_zip(file).map(|_| ()).map_err(io::Error::other))
}

fn save_with(folder: &Path, keep: u32, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    std::fs::create_dir_all(folder)?;

    // Keep the lock file in place so concurrent processes lock the same file
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(folder.join(LOCK_FILE))?;
    lock.lock()?;

    let path = next_path(folder)?;

    // Publish by rename so the numbered file is never visible during copying
    let options = WriteOptions::default().with_new_file_permissions(NewFilePermissions::Private);
    options.atomic_write_with(&path, write)?;
    prune(folder, keep, |path| std::fs::remove_file(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn retention_keeps_distinct_packages_and_ignores_directories() {
        let root = tempfile::tempdir().unwrap();
        let wood = Package::new("Wood");
        let first = folder(root.path(), Some("/first/wood.sbs"), 1, 0);
        let second = folder(root.path(), Some("/second/wood.sbs"), 1, 1);
        let untitled = folder(root.path(), None, 1, 0);
        assert_ne!(untitled, folder(root.path(), None, 2, 0));
        assert_ne!(untitled, folder(root.path(), None, 1, 1));
        assert_eq!(first, folder(root.path(), Some("/first/wood.sbs"), 2, 1));

        for _ in 0..5 {
            save(&first, &wood, 2).unwrap();
        }
        save(&second, &wood, 1).unwrap();
        std::fs::create_dir(first.join("99.mcraft")).unwrap();
        std::fs::rename(first.join("4.mcraft"), first.join("04.mcraft")).unwrap();
        save(&first, &wood, 2).unwrap();

        assert!(!first.join("04.mcraft").exists());
        assert!(first.join("5.mcraft").is_file());
        assert!(first.join("6.mcraft").is_file());
        assert!(first.join("99.mcraft").is_dir());
        assert!(second.join("1.mcraft").is_file());
        let archive = std::fs::read(first.join("6.mcraft")).unwrap();
        assert!(materialcraft_engine::formats::project::read(&archive, "autosave.mcraft").is_ok());
    }

    #[test]
    fn an_occupied_version_name_does_not_block_recovery() {
        let root = tempfile::tempdir().unwrap();
        let occupied = root.path().join("1.mcraft");
        std::fs::create_dir(&occupied).unwrap();

        save(root.path(), &Package::new("Wood"), 1).unwrap();

        assert!(occupied.is_dir());
        let bytes = std::fs::read(root.path().join("2.mcraft")).unwrap();
        assert!(materialcraft_engine::formats::project::read(&bytes, "autosave.mcraft").is_ok());
    }

    #[test]
    fn simultaneous_writes_publish_distinct_versions() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path();
        let barrier = std::sync::Barrier::new(4);

        std::thread::scope(|scope| {
            for number in 0..4 {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    let mut package = Package::new("Same filename");
                    package.graphs.push(materialcraft_engine::core::Graph::new(&format!("Graph {number}")));
                    save(folder, &package, 4).unwrap();
                });
            }
        });

        let mut names = Vec::new();
        for copy in copies(folder).unwrap() {
            let (_, filename) = copy.unwrap();
            let archive = std::fs::read(folder.join(filename)).unwrap();
            names.push(materialcraft_engine::formats::project::read(&archive, "autosave.mcraft").unwrap().pkg.graphs[0].name.clone());
        }

        names.sort_unstable();
        assert_eq!(names, ["Graph 0", "Graph 1", "Graph 2", "Graph 3"]);
    }

    #[test]
    fn a_truncated_legacy_copy_does_not_displace_a_complete_version() {
        let root = tempfile::tempdir().unwrap();
        let package = Package::new("Wood");
        save(root.path(), &package, 2).unwrap();
        let first = root.path().join("1.mcraft");
        let archive = std::fs::read(&first).unwrap();
        let incomplete = root.path().join("2.mcraft");
        std::fs::write(&incomplete, &archive[..archive.len() / 2]).unwrap();
        let embedded = root.path().join("3.mcraft");
        std::fs::write(&embedded, [&archive[..archive.len() / 2], &archive].concat()).unwrap();

        save(root.path(), &package, 2).unwrap();

        assert_eq!(std::fs::read(first).unwrap(), archive);
        assert!(incomplete.is_file());
        assert!(embedded.is_file());
        let latest = std::fs::read(root.path().join("4.mcraft")).unwrap();
        assert!(materialcraft_engine::formats::project::read(&latest, "autosave.mcraft").is_ok());
    }

    #[test]
    fn zip64_end_records_are_accepted() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("1.mcraft");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        writer.set_zip64_comment(Some(""));
        writer.start_file("package.sbs", zip::write::SimpleFileOptions::default()).unwrap();
        writer.finish().unwrap();

        assert!(has_recovery_footer(&path).unwrap());
    }

    #[test]
    fn pruning_continues_after_a_deletion_error() {
        let root = tempfile::tempdir().unwrap();
        let package = Package::new("Wood");
        for _ in 0..4 {
            save(root.path(), &package, 4).unwrap();
        }

        let error = prune(root.path(), 1, |path| {
            if path.ends_with("1.mcraft") {
                return Err(io::Error::new(ErrorKind::PermissionDenied, "injected deletion denial"));
            }

            std::fs::remove_file(path)
        })
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
        assert!(root.path().join("1.mcraft").is_file());
        assert!(!root.path().join("2.mcraft").exists());
        assert!(!root.path().join("3.mcraft").exists());
        assert!(root.path().join("4.mcraft").is_file());
    }

    #[test]
    fn overlapping_processes_keep_one_complete_copy_when_a_write_fails() {
        const CHILD_FOLDER: &str = "MATERIALCRAFT_AUTOSAVE_TEST_CHILD_FOLDER";
        const LOCK_CHECKED: &str = "lock-checked";
        const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
        if let Some(folder) = std::env::var_os(CHILD_FOLDER) {
            let folder = PathBuf::from(folder);
            let lock = OpenOptions::new().read(true).write(true).open(folder.join(LOCK_FILE)).unwrap();
            assert!(matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
            std::fs::write(folder.join(LOCK_CHECKED), []).unwrap();
            save(&folder, &Package::new("New"), 1).unwrap();
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let folder = root.path();
        save(folder, &Package::new("Original"), 1).unwrap();
        let first = folder.join("1.mcraft");
        let second = folder.join("2.mcraft");
        let mut child = None;

        let result = save_with(folder, 1, |file| {
            file.write_all(b"partial archive")?;
            let process = child.insert(
                Command::new(std::env::current_exe()?)
                    .args(["--exact", "autosave_versions::tests::overlapping_processes_keep_one_complete_copy_when_a_write_fails"])
                    .env(CHILD_FOLDER, folder)
                    .spawn()?,
            );
            let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
            while !folder.join(LOCK_CHECKED).exists() {
                if process.try_wait()?.is_some() || Instant::now() >= deadline {
                    return Err(io::Error::other("overlapping writer did not confirm lock contention"));
                }

                std::thread::sleep(Duration::from_millis(5));
            }

            assert!(first.is_file());
            assert!(!second.exists());
            Err(io::Error::new(ErrorKind::WriteZero, "injected interrupted write"))
        });

        let child_status = child.unwrap().wait().unwrap();
        assert_eq!(result.unwrap_err().kind(), ErrorKind::WriteZero);
        assert!(child_status.success());
        assert!(!first.exists());
        let archive = std::fs::read(second).unwrap();
        assert_eq!(materialcraft_engine::formats::project::read(&archive, "autosave.mcraft").unwrap().pkg.name, "New");
    }
}
