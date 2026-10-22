//! Embeds the default weapon and sound set TOMLs for `doomify-waw init`.
use std::{env, fs, path::Path};

fn main() {
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
    fs::write(out, format!("/// The default weapons and sound sets, by path.\npub const DEFAULTS: &[(&str, &str)] = &[\n{}\n];\n", entries.join("\n"))).unwrap();
}
