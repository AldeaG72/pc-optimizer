//! Compiles every recipe in `recipes/` into the binary, so `pco.exe` ships with exactly the
//! reviewed recipes and users cannot load their own (SPEC 5).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let recipes = manifest.join("..").join("..").join("recipes");
    println!("cargo:rerun-if-changed={}", recipes.display());

    let mut files = Vec::new();
    collect(&recipes, &mut files);
    files.sort();

    let mut code = String::from("pub static RECIPES: &[(&str, &str)] = &[\n");
    for file in &files {
        let relative = file
            .strip_prefix(&recipes)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let absolute = file.canonicalize().unwrap_or_else(|_| file.clone());
        let _ = writeln!(
            code,
            "    ({:?}, include_str!({:?})),",
            format!("recipes/{relative}"),
            absolute.to_string_lossy()
        );
    }
    code.push_str("];\n");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_default()).join("recipes.rs");
    if let Err(e) = fs::write(&out, code) {
        panic!("cannot write {}: {e}", out.display());
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        panic!("recipes folder not found: {}", dir.display());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"))
        {
            out.push(path);
        }
    }
}
