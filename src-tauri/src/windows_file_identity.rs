use std::{
    fs, io, mem::MaybeUninit, os::windows::fs::OpenOptionsExt, os::windows::io::AsRawHandle,
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE,
};

#[derive(Debug, PartialEq, Eq)]
struct FileIdentity {
    volume: u32,
    index: u64,
}

fn identity(file: &fs::File) -> io::Result<FileIdentity> {
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: File owns a live handle and the output points to writable storage.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: A successful call initialized the complete output structure.
    let information = unsafe { information.assume_init() };
    if information.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
    {
        return Err(io::Error::other("regular non-reparse file required"));
    }
    Ok(FileIdentity {
        volume: information.dwVolumeSerialNumber,
        index: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    })
}

pub(crate) fn same_file_handle(left: &fs::File, right: &fs::File) -> io::Result<bool> {
    Ok(identity(left)? == identity(right)?)
}

pub(crate) fn verify_named_file_handle(file: &fs::File, path: &Path) -> io::Result<()> {
    // Open the entry itself, so a reparse point cannot be followed between checking
    // its attributes and comparing identities. Deny deletion during the comparison.
    let named = fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if !same_file_handle(file, &named)? {
        return Err(io::Error::other("file path identity changed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn handles_distinguish_files_with_identical_contents() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("first");
        let other = temp.path().join("other");
        fs::write(&path, b"identical").unwrap();
        fs::write(&other, b"identical").unwrap();
        let first = fs::File::open(&path).unwrap();
        let reopened = fs::File::open(&path).unwrap();
        let different = fs::File::open(&other).unwrap();

        assert!(same_file_handle(&first, &reopened).unwrap());
        assert!(!same_file_handle(&first, &different).unwrap());
        verify_named_file_handle(&first, &path).unwrap();
        assert!(verify_named_file_handle(&first, &other).is_err());
    }

    #[test]
    fn hardlinks_have_the_same_identity() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("original");
        let link = temp.path().join("hardlink");
        fs::write(&path, b"content").unwrap();
        fs::hard_link(&path, &link).unwrap();
        let first = fs::File::open(&path).unwrap();
        let linked = fs::File::open(&link).unwrap();

        assert!(same_file_handle(&first, &linked).unwrap());
        verify_named_file_handle(&first, &link).unwrap();
    }

    #[test]
    fn named_symlink_is_rejected_even_when_it_targets_the_same_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("original");
        let link = temp.path().join("symlink");
        fs::write(&path, b"content").unwrap();
        if let Err(error) = std::os::windows::fs::symlink_file(&path, &link) {
            if error.raw_os_error() == Some(1314) {
                return; // Windows requires Developer Mode or symlink privileges.
            }
            panic!("could not create symlink: {error}");
        }
        let file = fs::File::open(&path).unwrap();
        assert!(verify_named_file_handle(&file, &link).is_err());
    }
}
