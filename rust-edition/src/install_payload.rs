//! File deployment for the native installer.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path};

/// Deploy a payload to a new installation directory. The caller supplies the
/// embedded files and their paths relative to the installation directory.
pub fn install_new(root: &Path, files: &[(&Path, &[u8])]) -> io::Result<()> {
    if files.is_empty()
        || files.iter().any(|(path, _)| {
            path.as_os_str().is_empty()
                || path.components().any(|part| {
                    !matches!(part, Component::Normal(name) if !name.to_string_lossy().contains(':'))
                })
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid installer payload",
        ));
    }

    fs::create_dir(root)?;
    let result = files.iter().try_for_each(|(path, bytes)| {
        let target = root.join(path);
        let parent = target.parent().expect("payload paths have a parent");
        fs::create_dir_all(parent)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        file.write_all(bytes)?;
        file.sync_all()
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(root);
    }
    result
}
