//! Plain UTF-8 programs. No file I/O or dialogs run in the audio callback.
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_FILE_BYTES: usize = 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileAction {
    Save,
    Load,
}

pub fn load(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Text files must be at most 1 MiB",
        ));
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "File is not UTF-8 text; save it as UTF-8 in your text editor",
        )
    })?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn save(path: &Path, code: &str) -> io::Result<()> {
    if code.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Programs must be at most 1 MiB",
        ));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut created = None;
    for _ in 0..32 {
        let temp = parent.join(format!(
            ".glicol-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => {
                created = Some((Temporary(temp), file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    let (temp, mut file) = created.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Cannot create a safe temporary save file",
        )
    })?;
    file.write_all(code.as_bytes())?;
    file.sync_all()?;
    drop(file);
    replace(&temp.0, path)
}

#[cfg(not(windows))]
fn replace(temp: &Path, target: &Path) -> io::Result<()> {
    fs::rename(temp, target)
}

#[cfg(windows)]
fn replace(temp: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "glicol-file-test-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn exact_utf8_roundtrip_overwrite_and_unicode_paths() {
        let dir = Directory::new();
        let path = dir.0.join("音 program.txt");
        let first = "o: speed 2.0 >> seq  55 60 _90 _ 48__90 >> mul 0.8";
        save(&path, first).unwrap();
        assert_eq!(load(&path).unwrap(), first);
        let next = "// café\r\no: sin 440 >> mul 0.05;\r\n";
        save(&path, next).unwrap();
        assert_eq!(load(&path).unwrap(), next);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn failed_save_keeps_existing_file_and_cleans_staging_file() {
        let dir = Directory::new();
        let path = dir.0.join("original.txt");
        save(&path, "original").unwrap();
        assert!(save(&path, &"x".repeat(MAX_FILE_BYTES + 1)).is_err());
        assert_eq!(load(&path).unwrap(), "original");
        // Replacement fails when the destination is a directory.
        assert!(save(&dir.0, "new").is_err());
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn utf8_bom_supported_but_invalid_missing_and_large_files_are_errors() {
        let dir = Directory::new();
        let path = dir.0.join("program.txt");
        fs::write(&path, "\u{feff}o: ~input;").unwrap();
        assert_eq!(load(&path).unwrap(), "o: ~input;");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(load(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::write(&path, vec![b'x'; MAX_FILE_BYTES + 1]).unwrap();
        assert_eq!(load(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(load(&dir.0.join("missing.txt")).is_err());
    }
}
