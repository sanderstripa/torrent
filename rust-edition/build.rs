use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-env-changed=TORRENT_INSTALL_PAYLOAD");
    let Some(out) = env::var_os("OUT_DIR") else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is unset"));
    };
    let mut entries = Vec::new();
    if env::var_os("CARGO_FEATURE_INSTALLER").is_some() {
        if let Some(root) = env::var_os("TORRENT_INSTALL_PAYLOAD") {
            let root = fs::canonicalize(root)?;
            collect(&root, &root, &mut entries)?;
        }
    }
    entries.sort_unstable();
    let mut generated = String::from("pub const FILES: &[(&str, &[u8])] = &[\n");
    for (path, file) in entries {
        generated.push_str(&format!("    ({path:?}, include_bytes!({file:?})),\n"));
    }
    generated.push_str("];\n");
    let out = PathBuf::from(out);
    fs::write(out.join("installer_payload.rs"), generated)?;
    #[cfg(windows)]
    {
        if env::var_os("CARGO_FEATURE_WINDOWS_APP").is_some() {
            write_resources(
                &out,
                "torrent-re-windows",
                "../assets/app.ico",
                "Torrent Rust Edition",
            )?;
        }
        if env::var_os("CARGO_FEATURE_INSTALLER").is_some() {
            write_resources(
                &out,
                "torrent-re-setup",
                "../installer/assets/installer.ico",
                "Torrent Rust Edition Setup",
            )?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn write_resources(out: &Path, binary: &str, icon: &str, description: &str) -> io::Result<()> {
    let icon = fs::canonicalize(icon)?;
    println!("cargo:rerun-if-changed={}", icon.display());
    // The SDK resource compiler accepts forward slashes and does not interpret them as escapes.
    let icon = icon.to_string_lossy().replace('\\', "/");
    let icon = icon.strip_prefix("//?/").unwrap_or(&icon);
    let script = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
 FILEVERSION 1,4,4,0
 PRODUCTVERSION 1,4,4,0
 FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "FileDescription", "{description}\0"
   VALUE "ProductName", "Torrent Rust Edition\0"
   VALUE "FileVersion", "1.4.4 RE\0"
   VALUE "ProductVersion", "1.4.4 RE\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#
    );
    let output = out.join(format!("{binary}.rc"));
    fs::write(&output, script)?;
    embed_resource::compile_for(&output, [binary], embed_resource::NONE)
        .manifest_optional()
        .map_err(|error| io::Error::other(error.to_string()))
}

fn collect(root: &Path, directory: &Path, entries: &mut Vec<(String, String)>) -> io::Result<()> {
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let ty = entry.file_type()?;
        if ty.is_dir() {
            collect(root, &path, entries)?;
        } else if ty.is_file() {
            let name = path
                .strip_prefix(root)
                .ok()
                .and_then(Path::to_str)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid payload path")
                })?;
            let absolute = path.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid payload source path")
            })?;
            println!("cargo:rerun-if-changed={absolute}");
            entries.push((name.to_owned(), absolute.to_owned()));
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid payload entry",
            ));
        }
    }
    Ok(())
}
