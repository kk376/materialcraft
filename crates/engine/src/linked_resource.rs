#[cfg(not(target_arch = "wasm32"))]
use std::io::Read as _;

/// Largest linked resource read from beside an SBS; matches the archive member cap.
#[cfg(not(target_arch = "wasm32"))]
const MAX_LINKED_RESOURCE_BYTES: u64 = 256 << 20;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_regular(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    use std::io::{Error, ErrorKind};
    let mut open = std::fs::OpenOptions::new();
    open.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        // A named pipe must reach the regular-file check without waiting for a writer
        open.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let file = open.open(path)?;
    let metadata = file.metadata()?;
    let invalid = || Error::new(ErrorKind::InvalidData, "expected a regular file of at most 256 MiB");
    if !metadata.is_file() || metadata.len() > MAX_LINKED_RESOURCE_BYTES {
        return Err(invalid());
    }

    let mut data = Vec::new();
    file.take(MAX_LINKED_RESOURCE_BYTES + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_LINKED_RESOURCE_BYTES {
        return Err(invalid());
    }

    Ok(data)
}

/// Read resource paths relative to an SBS package, including portable library aliases.
pub fn reader(path: &str) -> impl Fn(&str) -> Option<Vec<u8>> + use<> {
    let owner = path.to_string();
    #[cfg(not(target_arch = "wasm32"))]
    let folder = {
        let parent = std::path::Path::new(path).parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
        std::fs::canonicalize(parent).ok()
    };
    #[cfg(not(target_arch = "wasm32"))]
    let allowed_root = if materialcraft_formats::sbs::library::contains_file(std::path::Path::new(path)) {
        materialcraft_formats::sbs::library::root().canonicalize().ok()
    } else {
        folder.clone()
    };

    move |reference: &str| {
        if reference.starts_with("sbs://") {
            return materialcraft_formats::sbs::library::read(reference).ok();
        }

        if owner.starts_with("sbs://") {
            let uri = materialcraft_formats::sbs::library::resolve(&owner, reference).ok()?;
            return materialcraft_formats::sbs::library::read(&uri).ok();
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let relative = reference.trim().replace('\\', "/");
            let plain = !relative.is_empty() && !relative.contains(':') && !relative.starts_with('/');
            if !plain
                || std::path::Path::new(&relative)
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_) | std::path::Component::ParentDir | std::path::Component::CurDir))
            {
                return None;
            }

            let folder = folder.as_ref()?;
            let file = std::fs::canonicalize(folder.join(&relative)).ok()?;

            // Parent references are legal inside the configured library, never beyond its root
            if !file.starts_with(allowed_root.as_ref()?) {
                return None;
            }

            read_regular(&file).ok()
        }
        #[cfg(target_arch = "wasm32")]
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mc-linked-{}-{name}", std::process::id()))
    }

    #[test]
    fn linked_resources_over_the_cap_are_ignored() {
        let dir = temp("cap");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sbs = dir.join("p.sbs");
        std::fs::write(&sbs, b"x").unwrap();
        let big = dir.join("big.bin");
        std::fs::File::create(&big).unwrap().set_len(MAX_LINKED_RESOURCE_BYTES + 1).unwrap();

        let read = reader(&sbs.to_string_lossy());
        assert!(read("big.bin").is_none(), "a resource over the cap is not read");

        std::fs::write(dir.join("small.bin"), b"ok").unwrap();
        assert_eq!(read("small.bin").as_deref(), Some(&b"ok"[..]));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
