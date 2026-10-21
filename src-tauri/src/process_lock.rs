use fs2::FileExt;
use std::{fmt, fs, io, path::Path};

pub(crate) const APPLICATION_LOCK_NAME: &str = "pos-application.lock";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ApplicationLockError {
    Unavailable,
    Io,
}

impl fmt::Display for ApplicationLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("La aplicacion ya esta en ejecucion"),
            Self::Io => formatter.write_str("No se pudo establecer el bloqueo de la aplicacion"),
        }
    }
}

impl std::error::Error for ApplicationLockError {}

pub(crate) struct ApplicationLock {
    file: fs::File,
    path: std::path::PathBuf,
}

impl ApplicationLock {
    pub(crate) fn validate(&self) -> Result<(), ApplicationLockError> {
        lock_path_matches(&self.file, &self.path).map_err(|_| ApplicationLockError::Io)
    }
}

pub(crate) fn acquire(directory: &Path) -> Result<ApplicationLock, ApplicationLockError> {
    reject_symlink(directory).map_err(|_| ApplicationLockError::Io)?;
    let path = directory.join(APPLICATION_LOCK_NAME);
    let file = open_lock_file(&path).map_err(|_| ApplicationLockError::Io)?;
    secure_lock_file(&file).map_err(|_| ApplicationLockError::Io)?;
    FileExt::try_lock_exclusive(&file).map_err(|error| {
        // fs2 uses ERROR_LOCK_VIOLATION (33) on Windows. Its ErrorKind is
        // not necessarily WouldBlock; PermissionDenied and sharing failures
        // from opening the file must remain Io.
        if error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            && error.raw_os_error().is_some()
        {
            ApplicationLockError::Unavailable
        } else {
            ApplicationLockError::Io
        }
    })?;
    lock_path_matches(&file, &path).map_err(|_| ApplicationLockError::Io)?;
    Ok(ApplicationLock { file, path })
}

#[cfg(target_os = "linux")]
fn open_lock_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(0x20000) // O_NOFOLLOW
        .open(path)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn open_lock_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_lock_file(path: &Path) -> io::Result<fs::File> {
    #[cfg(windows)]
    use std::os::windows::fs::OpenOptionsExt;

    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(windows)]
    options
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(
            windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ
                | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE,
        );
    options.open(path)
}

#[cfg(unix)]
fn secure_lock_file(file: &fs::File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn secure_lock_file(_file: &fs::File) -> io::Result<()> {
    Ok(())
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other("symbolic link rejected"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("reparse point rejected"));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn lock_path_matches(file: &fs::File, path: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let opened = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if named.file_type().is_symlink() || opened.dev() != named.dev() || opened.ino() != named.ino()
    {
        return Err(io::Error::other("lock path identity changed"));
    }
    Ok(())
}

#[cfg(windows)]
fn lock_path_matches(file: &fs::File, path: &Path) -> io::Result<()> {
    crate::windows_file_identity::verify_named_file_handle(file, path)
}

#[cfg(not(any(unix, windows)))]
fn lock_path_matches(file: &fs::File, path: &Path) -> io::Result<()> {
    reject_symlink(path)?;
    if !file.metadata()?.is_file() || !fs::metadata(path)?.is_file() {
        return Err(io::Error::other("invalid lock file"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn lock_is_exclusive_for_the_process_lifetime() {
        let temp = TempDir::new().unwrap();
        let first = acquire(temp.path()).unwrap();

        assert!(matches!(
            acquire(temp.path()),
            Err(ApplicationLockError::Unavailable)
        ));

        drop(first);
        assert!(acquire(temp.path()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn replacing_the_named_lock_invalidates_the_guard() {
        let temp = TempDir::new().unwrap();
        let lock = acquire(temp.path()).unwrap();
        let path = temp.path().join(APPLICATION_LOCK_NAME);
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"replacement").unwrap();

        assert_eq!(lock.validate(), Err(ApplicationLockError::Io));
    }

    #[cfg(windows)]
    #[test]
    fn held_lock_cannot_be_deleted_or_replaced() {
        let temp = TempDir::new().unwrap();
        let lock = acquire(temp.path()).unwrap();
        let path = temp.path().join(APPLICATION_LOCK_NAME);
        let replacement = temp.path().join("replacement");
        fs::write(&replacement, b"replacement").unwrap();

        assert!(fs::remove_file(&path).is_err());
        assert!(fs::rename(&path, temp.path().join("moved")).is_err());
        assert!(fs::rename(&replacement, &path).is_err());
        lock.validate().unwrap();

        drop(lock);
        fs::remove_file(&path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_lock_violation_is_contention_but_sharing_and_access_denied_are_io() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(APPLICATION_LOCK_NAME);
        let first = acquire(temp.path()).unwrap();
        let second = open_lock_file(&path).unwrap();
        let error = FileExt::try_lock_exclusive(&second).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(33)); // ERROR_LOCK_VIOLATION
        assert_eq!(
            error.raw_os_error(),
            fs2::lock_contended_error().raw_os_error()
        );
        assert!(matches!(
            acquire(temp.path()),
            Err(ApplicationLockError::Unavailable)
        ));
        drop(second);
        drop(first);

        let denied_share = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert_eq!(open_lock_file(&path).unwrap_err().raw_os_error(), Some(32));
        assert!(matches!(
            acquire(temp.path()),
            Err(ApplicationLockError::Io)
        ));
        drop(denied_share);

        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions.clone()).unwrap();
        let denied = open_lock_file(&path).unwrap_err();
        let result = acquire(temp.path());
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
        assert_eq!(denied.raw_os_error(), Some(5)); // ERROR_ACCESS_DENIED
        assert!(matches!(result, Err(ApplicationLockError::Io)));
    }
}
