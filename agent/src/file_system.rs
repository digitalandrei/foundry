//! Filesystem confinement and local operations for volume file sessions.
//! All browser input is normalized relative to a controller-approved root;
//! symlinks are visible in listings but are never followed.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use foundry_shared::dto::{FileEntry, FileEntryKind, FileSessionRequest};
use foundry_shared::ServerVolumeId;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub(crate) const STORAGE_ROOT: &str = "/storage/containers";

/// Create and resolve a controller-approved volume root without following
/// symlinks in any component below `/storage/containers`. The returned
/// canonical path is what callers should retain for the rest of an operation.
pub(crate) async fn prepare_volume_root(path: &Path) -> Result<PathBuf, String> {
    validate_root_path(path)?;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || prepare_volume_root_sync(&path))
        .await
        .map_err(|error| format!("prepare volume root task failed: {error}"))?
}

/// Resolve an existing root for an operation that must not create it (storage
/// accounting and deletion). `None` means the volume has not been created on
/// this host yet; a symlink or non-directory component is always rejected.
pub(crate) fn existing_volume_root(path: &Path) -> Result<Option<PathBuf>, String> {
    validate_root_path(path)?;
    existing_volume_root_at(Path::new(STORAGE_ROOT), path)
}

pub(crate) async fn approved_roots(
    request: &FileSessionRequest,
) -> Result<HashMap<ServerVolumeId, PathBuf>, String> {
    let mut roots = HashMap::new();
    for volume in &request.volumes {
        let path = PathBuf::from(&volume.path);
        let canonical = prepare_volume_root(&path).await?;
        roots.insert(volume.volume_id, canonical);
    }
    Ok(roots)
}

fn validate_root_path(path: &Path) -> Result<(), String> {
    volume_components(Path::new(STORAGE_ROOT), path).map(|_| ())
}

fn prepare_volume_root_sync(path: &Path) -> Result<PathBuf, String> {
    prepare_volume_root_at(Path::new(STORAGE_ROOT), path)
}

fn prepare_volume_root_at(storage_root: &Path, path: &Path) -> Result<PathBuf, String> {
    let components = volume_components(storage_root, path)?;
    let storage_root = ensure_directory_tree(storage_root)?;
    let canonical_storage = std::fs::canonicalize(&storage_root)
        .map_err(|error| fs_error("resolve storage root", error))?;
    let mut current = storage_root;
    for component in components {
        current.push(component);
        ensure_directory(&current)?;
    }
    canonical_volume_root(&canonical_storage, &current)
}

fn existing_volume_root_at(storage_root: &Path, path: &Path) -> Result<Option<PathBuf>, String> {
    let components = volume_components(storage_root, path)?;
    let Some(storage_root) = existing_directory_tree(storage_root)? else {
        return Ok(None);
    };
    let canonical_storage = std::fs::canonicalize(&storage_root)
        .map_err(|error| fs_error("resolve storage root", error))?;
    let mut current = storage_root;
    for component in components {
        current.push(component);
        let Some(directory) = existing_directory(&current)? else {
            return Ok(None);
        };
        current = directory;
    }
    canonical_volume_root(&canonical_storage, &current).map(Some)
}

fn volume_components(storage_root: &Path, path: &Path) -> Result<Vec<OsString>, String> {
    let relative = path.strip_prefix(storage_root).map_err(|_| {
        format!(
            "refusing volume root outside {}: {}",
            storage_root.display(),
            path.display()
        )
    })?;
    let components: Vec<OsString> = relative
        .components()
        .map(|component| match component {
            Component::Normal(component) => Ok(component.to_os_string()),
            _ => Err(format!(
                "refusing volume root outside {}: {}",
                storage_root.display(),
                path.display()
            )),
        })
        .collect::<Result<_, _>>()?;
    if components.is_empty() {
        return Err(format!(
            "refusing volume root outside {}: {}",
            storage_root.display(),
            path.display()
        ));
    }
    Ok(components)
}

fn ensure_directory_tree(path: &Path) -> Result<PathBuf, String> {
    let mut current = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(component) => {
                current.push(component);
                ensure_directory(&current)?;
            }
            _ => return Err(format!("invalid storage root: {}", path.display())),
        }
    }
    Ok(current)
}

fn existing_directory_tree(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut current = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(component) => {
                current.push(component);
                let Some(directory) = existing_directory(&current)? else {
                    return Ok(None);
                };
                current = directory;
            }
            _ => return Err(format!("invalid storage root: {}", path.display())),
        }
    }
    Ok(Some(current))
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => ensure_real_directory(path, metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(fs_error("create volume directory", error)),
            }
            let metadata = std::fs::symlink_metadata(path)
                .map_err(|error| fs_error("inspect volume directory", error))?;
            ensure_real_directory(path, metadata)
        }
        Err(error) => Err(fs_error("inspect volume directory", error)),
    }
}

fn existing_directory(path: &Path) -> Result<Option<PathBuf>, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure_real_directory(path, metadata)?;
            Ok(Some(path.to_path_buf()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(fs_error("inspect volume directory", error)),
    }
}

fn ensure_real_directory(path: &Path, metadata: std::fs::Metadata) -> Result<(), String> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "volume path {} is not a real directory",
            path.display()
        ));
    }
    Ok(())
}

fn canonical_volume_root(storage_root: &Path, path: &Path) -> Result<PathBuf, String> {
    let canonical =
        std::fs::canonicalize(path).map_err(|error| fs_error("resolve volume root", error))?;
    if !canonical.starts_with(storage_root) || canonical == storage_root {
        return Err(format!(
            "refusing volume root outside {}: {}",
            storage_root.display(),
            path.display()
        ));
    }
    Ok(canonical)
}

pub(crate) fn root(
    roots: &HashMap<ServerVolumeId, PathBuf>,
    volume_id: ServerVolumeId,
) -> Result<&PathBuf, String> {
    roots
        .get(&volume_id)
        .ok_or_else(|| "volume is not approved for this session".to_string())
}

fn normalize_relative(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".into());
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("path traversal is not allowed".into())
            }
        }
    }
    Ok(clean)
}

/// Resolve an existing entry without following symlinks in any component.
pub(crate) fn resolve_existing(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let relative = normalize_relative(raw)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        let metadata =
            std::fs::symlink_metadata(&current).map_err(|error| fs_error("inspect path", error))?;
        if metadata.file_type().is_symlink() {
            return Err("symlinks cannot be followed by the volume browser".into());
        }
    }
    Ok(current)
}

pub(crate) fn non_root_existing(root: &Path, raw: &str) -> Result<PathBuf, String> {
    if normalize_relative(raw)?.as_os_str().is_empty() {
        return Err("the volume root itself cannot be changed".into());
    }
    resolve_existing(root, raw)
}

/// Resolve a destination whose parent must already exist and be symlink-free.
pub(crate) fn resolve_destination(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let relative = normalize_relative(raw)?;
    let name = relative
        .file_name()
        .ok_or_else(|| "a file or directory name is required".to_string())?
        .to_owned();
    let parent_rel = relative.parent().unwrap_or_else(|| Path::new(""));
    let parent = resolve_existing(root, &parent_rel.to_string_lossy())?;
    let destination = parent.join(name);
    if let Ok(metadata) = std::fs::symlink_metadata(&destination) {
        if metadata.file_type().is_symlink() {
            return Err("symlink destinations are not allowed".into());
        }
    }
    Ok(destination)
}

pub(crate) fn resolve_new_destination(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let destination = resolve_destination(root, raw)?;
    if std::fs::symlink_metadata(&destination).is_ok() {
        return Err("destination already exists".into());
    }
    Ok(destination)
}

/// Deleting a symlink is safe; only its parent is resolved and confined.
pub(crate) fn resolve_for_delete(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let relative = normalize_relative(raw)?;
    let name = relative
        .file_name()
        .ok_or_else(|| "the volume root itself cannot be deleted".to_string())?
        .to_owned();
    let parent_rel = relative.parent().unwrap_or_else(|| Path::new(""));
    let parent = resolve_existing(root, &parent_rel.to_string_lossy())?;
    let target = parent.join(name);
    std::fs::symlink_metadata(&target).map_err(|error| fs_error("inspect path", error))?;
    Ok(target)
}

pub(crate) fn list_directory(root: &Path, raw: &str) -> Result<Vec<FileEntry>, String> {
    let directory = resolve_existing(root, raw)?;
    if !directory.is_dir() {
        return Err("path is not a directory".into());
    }
    let relative = normalize_relative(raw)?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|error| fs_error("read directory", error))? {
        let entry = entry.map_err(|error| fs_error("read directory entry", error))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF-8 file names are not supported".to_string())?;
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|error| fs_error("inspect directory entry", error))?;
        let kind = if metadata.file_type().is_symlink() {
            FileEntryKind::Symlink
        } else if metadata.is_dir() {
            FileEntryKind::Directory
        } else {
            FileEntryKind::File
        };
        let path = relative.join(&name).to_string_lossy().to_string();
        entries.push(FileEntry {
            name,
            path,
            kind,
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
            modified_at: metadata.modified().ok().map(chrono::DateTime::from),
        });
    }
    entries.sort_by(|a, b| {
        let a_dir = a.kind == FileEntryKind::Directory;
        let b_dir = b.kind == FileEntryKind::Directory;
        b_dir
            .cmp(&a_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub(crate) fn copy_entry(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.starts_with(source) {
        return Err("cannot copy a directory into itself".into());
    }
    let metadata =
        std::fs::symlink_metadata(source).map_err(|error| fs_error("inspect source", error))?;
    if metadata.file_type().is_symlink() {
        return Err("symlinks cannot be copied by the volume browser".into());
    }
    if metadata.is_dir() {
        std::fs::create_dir(destination)
            .map_err(|error| fs_error("create destination directory", error))?;
        for entry in
            std::fs::read_dir(source).map_err(|error| fs_error("read source directory", error))?
        {
            let entry = entry.map_err(|error| fs_error("read source entry", error))?;
            copy_entry(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        std::fs::copy(source, destination).map_err(|error| fs_error("copy file", error))?;
    } else {
        return Err("only regular files and directories can be copied".into());
    }
    Ok(())
}

pub(crate) fn move_entry(source: &Path, destination: &Path) -> Result<(), String> {
    if std::fs::rename(source, destination).is_ok() {
        return Ok(());
    }
    copy_entry(source, destination)?;
    delete_entry(source)
}

pub(crate) fn delete_entry(target: &Path) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(target).map_err(|error| fs_error("inspect target", error))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(target).map_err(|error| fs_error("delete directory", error))
    } else {
        std::fs::remove_file(target).map_err(|error| fs_error("delete file", error))
    }
}

pub(crate) async fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = temporary_sibling(destination, Uuid::now_v7())?;
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .await
        .map_err(|error| fs_error("create temporary file", error))?;
    file.write_all(bytes)
        .await
        .map_err(|error| fs_error("write temporary file", error))?;
    file.flush()
        .await
        .map_err(|error| fs_error("flush temporary file", error))?;
    drop(file);
    if let Err(error) = tokio::fs::rename(&temporary, destination).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(fs_error("replace file", error));
    }
    Ok(())
}

pub(crate) fn temporary_sibling(destination: &Path, request_id: Uuid) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "destination has no parent".to_string())?;
    Ok(parent.join(format!(".foundry-upload-{request_id}")))
}

pub(crate) fn fs_error(action: &str, error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => {
            format!("{action}: permission denied (upgrade the agent setup if this persists)")
        }
        std::io::ErrorKind::NotFound => format!("{action}: path no longer exists"),
        std::io::ErrorKind::AlreadyExists => format!("{action}: destination already exists"),
        _ => format!("{action}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        existing_volume_root_at, normalize_relative, prepare_volume_root_at, resolve_existing,
        temporary_sibling, validate_root_path,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn paths_are_relative_and_cannot_traverse() {
        assert!(normalize_relative("").is_ok());
        assert!(normalize_relative("models/checkpoints").is_ok());
        assert!(normalize_relative("../etc/shadow").is_err());
        assert!(normalize_relative("/etc/shadow").is_err());
    }

    #[test]
    fn approved_roots_stay_below_storage() {
        assert!(validate_root_path(Path::new("/storage/containers/volumes/019abc")).is_ok());
        assert!(validate_root_path(Path::new("/storage/containers")).is_err());
        assert!(validate_root_path(Path::new("/etc")).is_err());
        assert!(validate_root_path(Path::new("/storage/containers/../etc")).is_err());
    }

    #[test]
    fn prepare_volume_root_rejects_a_symlinked_component() {
        use std::os::unix::fs::symlink;

        let sandbox = temporary_root("prepare-symlink");
        let storage = sandbox.join("storage/containers");
        let outside = sandbox.join("outside");
        std::fs::create_dir_all(&storage).unwrap();
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, storage.join("escape")).unwrap();

        let error = prepare_volume_root_at(&storage, &storage.join("escape/volume"))
            .expect_err("a volume root cannot traverse a symlink");

        assert!(error.contains("not a real directory"));
        assert!(!outside.join("volume").exists());
        std::fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn prepare_volume_root_creates_only_real_directories() {
        let sandbox = temporary_root("prepare-real");
        let storage = sandbox.join("storage/containers");
        let target = storage.join(".foundry/slots/slot-a/name-a/volume-a");

        let prepared = prepare_volume_root_at(&storage, &target).expect("root is prepared");

        assert!(prepared.is_dir());
        assert_eq!(prepared, std::fs::canonicalize(&target).unwrap());
        std::fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn deletion_validation_refuses_a_symlinked_root_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let sandbox = temporary_root("delete-symlink");
        let storage = sandbox.join("storage/containers");
        let outside = sandbox.join("outside");
        std::fs::create_dir_all(&storage).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let sentinel = outside.join("keep.txt");
        std::fs::write(&sentinel, b"keep").unwrap();
        let root = storage.join("volume");
        symlink(&outside, &root).unwrap();

        let error = existing_volume_root_at(&storage, &root)
            .expect_err("a deletion target cannot be a symlink");

        assert!(error.contains("not a real directory"));
        assert!(root.is_symlink());
        assert!(sentinel.exists());
        std::fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn existing_path_resolution_refuses_symlink_components() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("foundry-files-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root).unwrap();
        symlink("/etc", root.join("escape")).unwrap();
        let error = resolve_existing(&root, "escape/passwd").unwrap_err();
        assert!(error.contains("symlink"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upload_request_id_selects_a_stable_partial_file() {
        let id = uuid::Uuid::now_v7();
        let destination = Path::new("/storage/containers/volumes/one/model.bin");
        let first = temporary_sibling(destination, id).unwrap();
        let resumed = temporary_sibling(destination, id).unwrap();
        let expected = format!(".foundry-upload-{id}");
        assert_eq!(first, resumed);
        assert_eq!(
            first.file_name().and_then(|name| name.to_str()),
            Some(expected.as_str())
        );
    }

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("foundry-files-{label}-{}", uuid::Uuid::now_v7()))
    }
}
