//! Embeds the default weapon and sound set TOMLs (`DEFAULTS`), and makes `FINGERPRINT`, a mark
//! of doomify's own source (its Rust, the render script and the scene), so a program built with
//! it knows when what it builds may have changed.
use std::{env, fs, path::Path};

/// Every file under `folder`, sorted.
fn files(folder: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

fn main() {
    let mut sources = Vec::new();
    for folder in ["src", "blender"] {
        println!("cargo:rerun-if-changed={folder}");
        files(Path::new(folder), &mut sources);
    }
    sources.sort();
    let mut hash = 0xcbf29ce484222325u64;
    for source in sources.iter().filter(|path| {
        path.extension()
            .is_some_and(|extension| ["rs", "py", "blend"].contains(&&*extension.to_string_lossy()))
    }) {
        for byte in fs::read(source).unwrap_or_default() {
            hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
        }
    }

    let mut entries = vec![];
    for folder in ["weapons", "sounds"] {
        println!("cargo:rerun-if-changed={folder}");
        let mut names: Vec<_> = fs::read_dir(folder)
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        for name in names.into_iter().filter(|n| n.ends_with(".toml")) {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(folder)
                .join(&name);
            entries.push(format!(
                "    (\"{folder}/{name}\", include_str!({:?})),",
                path.display().to_string()
            ));
        }
    }
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("defaults.rs");
    fs::write(out, format!("/// The default weapons and sound sets, by path.\npub const DEFAULTS: &[(&str, &str)] = &[\n{}\n];\n/// A mark of doomify's own source: what it builds may differ when it changes.\npub const FINGERPRINT: &str = \"{hash:016x}\";\n", entries.join("\n"))).unwrap();
}
