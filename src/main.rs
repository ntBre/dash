#![feature(lazy_cell)]

use std::{
    fs::{create_dir_all, remove_dir_all},
    io::{self},
    path::Path,
    process::Command,
    sync::LazyLock,
};

use gui::MyApp;

mod gui;
mod project;

/// create a temporary directory and return its path
fn tempdir() -> io::Result<std::path::PathBuf> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let dir = base.join(format!("dash.{pid}"));
    create_dir_all(&dir)?;
    Ok(dir)
}

static DEBUG: LazyLock<bool> = LazyLock::new(|| std::env::var("DEBUG").is_ok());
const PROGRAM_TITLE: &str = "dash";

fn config_file() -> std::path::PathBuf {
    let home = match std::env::var("HOME") {
        Ok(v) => v,
        Err(_) => panic!("no input file supplied and unable to read $HOME"),
    };

    Path::new(&home)
        .join(".config")
        .join(PROGRAM_TITLE)
        .join("config.toml")
}

fn main() -> anyhow::Result<()> {
    let temp = tempdir()?;

    let args: Vec<_> = std::env::args().collect();

    let infile = if args.len() == 2 {
        if args[1] == "edit" {
            let editor =
                std::env::var("EDITOR").unwrap_or_else(|_| String::from("vim"));
            let conf = config_file();
            Command::new(editor).arg(conf).status()?;
            return Ok(());
        }
        Path::new(&args[1]).to_path_buf()
    } else {
        let config = config_file();
        if !config.exists() {
            panic!("no input file supplied and none found at {config:?}");
        }
        config
    };

    let projects = project::Project::load(infile, &temp)?;

    let app = MyApp::new(temp.clone(), projects);

    eframe::run_native(
        PROGRAM_TITLE,
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::new(app)),
    )
    .unwrap();

    match remove_dir_all(temp) {
        Ok(_) => (),
        Err(e) => eprintln!("{e}"),
    }
    Ok(())
}
