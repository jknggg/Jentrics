use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap();
    let cwd = std::env::current_dir().unwrap();
    writeln!(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("calls.jsonl"))
            .unwrap(),
        "{}",
        json!({"args":args,"cwd":cwd,"cwdEntries":fs::read_dir(&cwd).unwrap().count()})
    )
    .unwrap();
    if args == ["--version"] {
        match fs::read_to_string(dir.join("mode.txt")).unwrap().as_str() {
            "preflight-stdout-limit" => {
                std::io::stdout().write_all(&vec![b'x'; 100_000]).unwrap();
                return;
            }
            "preflight-stderr-limit" => {
                std::io::stderr().write_all(&vec![b'x'; 100_000]).unwrap();
                return;
            }
            "preflight-timeout" => std::thread::sleep(std::time::Duration::from_secs(30)),
            _ => {}
        }
        println!("{}", fs::read_to_string(dir.join("version.txt")).unwrap());
    } else if args.last().is_some_and(|arg| arg == "--help") {
        println!("{}", fs::read_to_string(dir.join("help.txt")).unwrap());
    } else if args.windows(2).any(|args| args == ["debug", "models"]) {
        println!("{}", fs::read_to_string(dir.join("catalog.json")).unwrap());
    } else if args.windows(2).any(|args| args == ["login", "status"]) {
        let status = fs::read_to_string(dir.join("auth.txt")).unwrap();
        eprintln!("{status}");
        if status == "Not logged in" {
            std::process::exit(1);
        }
    } else if args.first().is_some_and(|arg| arg == "--no-daemon")
        && args.iter().any(|arg| arg == "exec")
    {
        fs::write(dir.join("model-started.txt"), "started").unwrap();
        let catalog = args
            .windows(2)
            .find(|pair| pair[0] == "-c" && pair[1].starts_with("model_catalog_json="))
            .unwrap();
        let path: String =
            serde_json::from_str(catalog[1].strip_prefix("model_catalog_json=").unwrap()).unwrap();
        fs::copy(&path, dir.join("observed-catalog.json")).unwrap();
        let mode = fs::read_to_string(dir.join("mode.txt")).unwrap();
        match mode.as_str() {
            "stdout-limit" => {
                std::io::stdout().write_all(&vec![b'x'; 200_000]).unwrap();
            }
            "stderr-limit" => {
                std::io::stderr().write_all(&vec![b'x'; 200_000]).unwrap();
            }
            "sleep" => std::thread::sleep(std::time::Duration::from_secs(120)),
            _ => print!("{}", fs::read_to_string(dir.join("events.jsonl")).unwrap()),
        }
        if mode == "nonzero" {
            std::process::exit(1);
        }
    } else {
        fs::write(
            dir.join("model-started.txt"),
            "unexpected provider operation",
        )
        .unwrap();
        std::process::exit(77);
    }
}
