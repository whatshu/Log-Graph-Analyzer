mod tui;

use std::path::PathBuf;

fn main() {
    let mut workspace_root = PathBuf::from(".logrepo");
    let mut initial_repo: Option<String> = None;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-w" | "--workspace" => {
                i += 1;
                if i < args.len() {
                    workspace_root = PathBuf::from(&args[i]);
                }
            }
            "-r" | "--repo" => {
                i += 1;
                if i < args.len() {
                    initial_repo = Some(args[i].clone());
                }
            }
            _ => {}
        }
        i += 1;
    }

    if let Err(e) = tui::run(&workspace_root, initial_repo.as_deref()) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
