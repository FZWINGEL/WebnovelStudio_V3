use super::*;

pub(crate) fn install_project_directory(staging: &Path, destination: &Path) -> CoreResult<()> {
    // The existence check alone cannot prevent replacing a directory that
    // appears between the check and rename. Use the OS no-replace operation.
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        renameat_with(CWD, staging, CWD, destination, RenameFlags::NOREPLACE)
            .map_err(|e| CoreError::from(std::io::Error::from(e)))?;
        File::open(
            destination
                .parent()
                .ok_or_else(|| CoreError::new("InvalidRequest", "Missing destination parent."))?,
        )?
        .sync_all()?;
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let wide = |path: &Path| -> CoreResult<Vec<u16>> {
            let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
            if units.contains(&0) {
                return Err(CoreError::new(
                    "InvalidRequest",
                    "A project path cannot contain NUL.",
                ));
            }
            units.push(0);
            Ok(units)
        };
        let from = wide(staging)?;
        let to = wide(destination)?;
        // SAFETY: both buffers are owned, NUL-terminated UTF-16 and remain live
        // for this synchronous call. REPLACE_EXISTING is deliberately absent.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (staging, destination);
        Err(CoreError::new(
            "UnsupportedPlatform",
            "Safe project installation has not been qualified on this platform.",
        ))
    }
}

#[test]
fn installation_refuses_an_empty_directory_that_appeared_late() {
    let root = std::env::temp_dir().join(format!("wns-install-test-{}", new_id()));
    std::fs::create_dir(&root).unwrap();
    let staging = root.join("staging");
    let destination = root.join("existing");
    std::fs::create_dir(&staging).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(staging.join("prose.txt"), "retained").unwrap();
    assert!(install_project_directory(&staging, &destination).is_err());
    assert!(staging.join("prose.txt").exists());
    assert!(std::fs::read_dir(&destination).unwrap().next().is_none());
    assert!(
        root.starts_with(std::env::temp_dir())
            && root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wns-install-test-")
    );
    std::fs::remove_dir_all(root).unwrap();
}

pub(crate) fn write_project_marker(path: &Path, info: &ProjectInfo) -> CoreResult<()> {
    let marker = path.join("project.wns.json");
    let temporary = path.join(format!(".project.wns.json-{}", new_id()));
    let result = (|| -> CoreResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(info)?)?;
        file.sync_all()?;
        drop(file);

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            };
            let wide = |value: &Path| -> CoreResult<Vec<u16>> {
                let mut units: Vec<u16> = value.as_os_str().encode_wide().collect();
                if units.contains(&0) {
                    return Err(CoreError::new(
                        "InvalidRequest",
                        "A project path cannot contain NUL.",
                    ));
                }
                units.push(0);
                Ok(units)
            };
            let from = wide(&temporary)?;
            let to = wide(&marker)?;
            // SAFETY: both buffers are owned, NUL-terminated UTF-16 and stay
            // live for this synchronous call.
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        #[cfg(not(windows))]
        std::fs::rename(&temporary, &marker)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
