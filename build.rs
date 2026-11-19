//! Intègre au binaire les tickets d'exemple du site (docs/exemples/*.png et en/*.png) : le
//! catalogue de l'appli les montre en vignette. Et les planches des coloriages (assets/coloring/).

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = String::from("// Généré par build.rs : tickets d'exemple de docs/exemples/.\n");
    for (name, dir) in [("EXAMPLES", "docs/exemples"), ("EXAMPLES_EN", "docs/exemples/en")] {
        let dir = root.join(dir);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect()
            })
            .unwrap_or_default();
        files.sort();
        writeln!(out, "pub static {name}: &[(&str, &[u8])] = &[").unwrap();
        for path in files {
            let id = path.file_stem().unwrap().to_string_lossy();
            writeln!(out, "    ({id:?}, include_bytes!({:?})),", path.display().to_string()).unwrap();
        }
        writeln!(out, "];").unwrap();
    }
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("examples.rs");
    std::fs::write(dest, out).unwrap();
    coloring(root);
}

/// Intègre les planches des coloriages (assets/coloring/*.png), décrites par
/// assets/coloring/planches.json.
fn coloring(root: &Path) {
    let dir = root.join("assets/coloring");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut out = String::from("// Généré par build.rs : planches de assets/coloring/.\n");
    writeln!(out, "pub static SHEETS: &[(&str, &[u8])] = &[").unwrap();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy();
        writeln!(out, "    ({name:?}, include_bytes!({:?})),", path.display().to_string()).unwrap();
    }
    writeln!(out, "];").unwrap();
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("coloring.rs");
    std::fs::write(dest, out).unwrap();
}
