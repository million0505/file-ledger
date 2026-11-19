use file_ledger::{diff, duplicates, load, scan::scan, write_new};
use serde::Serialize;
use std::path::Path;

fn print_json(value: &impl Serialize) -> Result<(), String> {
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut stream = stdout.lock();
    serde_json::to_writer_pretty(&mut stream, value).map_err(|e| e.to_string())?;
    writeln!(stream).map_err(|e| e.to_string())
}

fn run(args: &[String]) -> Result<u8, String> {
    match args {
        [command] if command == "--version" => {
            println!("file-ledger {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        [command] if command == "--help" || command == "help" => {
            println!("file-ledger — read-only SHA-256 folder audit\n\n  scan ROOT [--out MANIFEST.json]\n  verify MANIFEST.json ROOT\n  diff BEFORE.json AFTER.json\n  duplicates MANIFEST.json\n\nJSON on stdout. Exit 0: success/unchanged; 1: differences; 2: errors/incomplete.\nManifest output must be outside ROOT and must not already exist.\nNo delete, rename, upload, or repair commands.");
            Ok(0)
        }
        [command, root] if command == "scan" => {
            let manifest = scan(Path::new(root))?;
            print_json(&manifest)?;
            Ok(if manifest.errors.is_empty() { 0 } else { 2 })
        }
        [command, root, flag, output] if command == "scan" && flag == "--out" => {
            let manifest = scan(Path::new(root))?;
            write_new(Path::new(output), &manifest, Path::new(root))?;
            print_json(
                &serde_json::json!({"output":output,"files":manifest.files.len(),"errors":manifest.errors.len(),"skipped":manifest.skipped.len()}),
            )?;
            Ok(if manifest.errors.is_empty() { 0 } else { 2 })
        }
        [command, path] if command == "duplicates" => {
            print_json(&duplicates(&load(Path::new(path))?)?)?;
            Ok(0)
        }
        [command, before, after] if command == "diff" => {
            let result = diff(&load(Path::new(before))?, &load(Path::new(after))?)?;
            print_json(&result)?;
            Ok(if result.is_clean() { 0 } else { 1 })
        }
        [command, manifest, root] if command == "verify" => {
            let result = diff(&load(Path::new(manifest))?, &scan(Path::new(root))?)?;
            print_json(&result)?;
            Ok(if result.is_clean() { 0 } else { 1 })
        }
        _ => Err("invalid command; run file-ledger --help".into()),
    }
}

fn main() -> std::process::ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("file-ledger: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
