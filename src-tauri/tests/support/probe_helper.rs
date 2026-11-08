use std::{
    io::{self, BufRead, Write},
    time::Duration,
};

// Intentionally leave a descendant for the runner's job-object cleanup test.
#[allow(clippy::zombie_processes)]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--version") => {
            // Only the explicitly launched native trace fixture sets this.
            if let Some(delay) = std::env::var_os("JENTRICS_PROBE_HELPER_VERSION_DELAY_MS") {
                let delay: u64 = delay.to_str().unwrap().parse().unwrap();
                assert!(delay <= 3000);
                std::thread::sleep(Duration::from_millis(delay));
            }
            println!(
                "{}",
                std::fs::read_to_string("version.txt").expect("version fixture is required")
            );
        }
        Some("app-server") if args.get(2).is_some_and(|a| a == "--help") => {
            println!(
                "{}",
                std::fs::read_to_string("help.txt").expect("help fixture is required")
            )
        }
        Some("app-server") if !std::path::Path::new("account-result.json").exists() => {
            std::fs::write("app-server-started.txt", "started").unwrap();
            println!("malformed response");
        }
        Some("sleep") => {
            if let Some(path) = args.get(2) {
                std::fs::write(path, std::process::id().to_string()).unwrap();
            }
            std::thread::sleep(Duration::from_secs(30));
        }
        Some("stdout") => io::stdout().write_all(&vec![b'x'; 100_000]).unwrap(),
        Some("stderr") => io::stderr().write_all(&vec![b'x'; 100_000]).unwrap(),
        Some("echo") => println!("{}", args.get(2).unwrap()),
        Some("descendant") => {
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("sleep")
                .spawn()
                .unwrap();
            println!("{}", child.id());
            if let Some(path) = args.get(2) {
                std::fs::write(path, child.id().to_string()).unwrap();
            }
            io::stdout().flush().unwrap();
            std::thread::sleep(Duration::from_secs(30));
        }
        Some("detached-descendant") => {
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("sleep")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            println!("{}", child.id());
        }
        Some("rpc") | Some("app-server") => {
            std::fs::write("app-server-started.txt", "started").unwrap();
            let mut initialized = false;
            let mut limits_seen = false;
            for line in io::stdin().lock().lines() {
                let msg: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
                let method = msg["method"].as_str().unwrap();
                use std::fs::OpenOptions;
                writeln!(
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("rpc-methods.txt")
                        .unwrap(),
                    "{method}"
                )
                .unwrap();
                match method {
                    "initialize" => println!("{{\"id\":0,\"result\":{{}}}}"),
                    "initialized" => initialized = true,
                    "account/read" => {
                        assert!(initialized);
                        assert_eq!(msg["params"]["refreshToken"], false);
                        let result: serde_json::Value = serde_json::from_str(
                            &std::fs::read_to_string("account-result.json")
                                .expect("account fixture is required"),
                        )
                        .expect("account fixture must be valid JSON");
                        println!("{}", serde_json::json!({"id":1,"result":result}));
                    }
                    "account/rateLimits/read" => {
                        assert!(initialized);
                        assert!(!limits_seen);
                        limits_seen = true;
                        let mut response: serde_json::Value = serde_json::from_str(
                            &std::fs::read_to_string("limits-response.json")
                                .expect("limits fixture is required"),
                        )
                        .unwrap();
                        if response["fixtureMode"] == "sleep" {
                            std::thread::sleep(Duration::from_secs(30));
                        }
                        response["id"] = serde_json::json!(2);
                        println!("{response}");
                    }
                    "account/usage/read" => {
                        assert!(initialized && limits_seen);
                        assert!(msg.get("params").is_none());
                        let mut response: serde_json::Value = serde_json::from_slice(
                            &std::fs::read("usage-response.json").expect("usage fixture required"),
                        )
                        .unwrap();
                        response["id"] = serde_json::json!(3);
                        println!("{response}");
                    }
                    _ => panic!("unexpected method"),
                }
                io::stdout().flush().unwrap();
            }
        }
        _ => {}
    }
}
