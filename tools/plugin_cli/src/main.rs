//! drafftink-plugin — command-line plugin manager.
//!
//! Usage:
//!   drafftink-plugin install <path-to-plugin.dll>
//!   drafftink-plugin uninstall <name>
//!   drafftink-plugin list

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        help();
        return;
    }

    let dir = plugins_dir();

    match args[1].as_str() {
        "install" => install(&dir, args.get(2)),
        "uninstall" | "remove" => uninstall(&dir, args.get(2)),
        "list" | "ls" => list(&dir),
        _ => help(),
    }
}

fn help() {
    println!("drafftink-plugin — Plugin Manager\n");
    println!("  install <plugin.dll>    Install a plugin");
    println!("  uninstall <name>         Remove a plugin");
    println!("  list                     Show installed plugins");
}

fn plugins_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    #[cfg(not(windows))]
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(base).join("drafftink").join("plugins");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn install(dir: &Path, src: Option<&String>) {
    let src = match src {
        Some(s) => s,
        None => {
            eprintln!("Usage: install <plugin.dll>");
            return;
        }
    };
    let src_path = PathBuf::from(src);
    if !src_path.exists() {
        eprintln!("[error] File not found: {}", src);
        return;
    }
    let name = src_path.file_name().unwrap_or_default();
    let dest = dir.join(name);
    match fs::copy(&src_path, &dest) {
        Ok(_) => println!("[ok] Installed {:?}", dest),
        Err(e) => eprintln!("[error] Copy failed: {}", e),
    }
}

fn uninstall(dir: &PathBuf, name: Option<&String>) {
    let name = match name {
        Some(n) => n,
        None => {
            eprintln!("Usage: uninstall <name>");
            return;
        }
    };
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.file_stem().map(|s| s == name.as_str()).unwrap_or(false) {
                let _ = fs::remove_file(&p);
                println!("[ok] Uninstalled {:?}", p);
                return;
            }
        }
    }
    println!("[info] Plugin '{}' not found", name);
}

fn list(dir: &PathBuf) {
    println!("Plugins in {:?}:\n", dir);
    let mut any = false;
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if matches!(ext, "dll" | "so" | "dylib") {
                    let sz = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    println!(
                        "  {}  ({:.1} KB)",
                        p.file_stem().unwrap_or_default().to_string_lossy(),
                        sz as f64 / 1024.0,
                    );
                    any = true;
                }
            }
        }
    }
    if !any {
        println!("  (none)");
    }
}
