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
    fs::write(PathBuf::from(out).join("installer_payload.rs"), generated)
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
