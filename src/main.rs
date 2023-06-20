use std::{
    fs::{create_dir_all, read_to_string, remove_dir_all},
    io::{self, Read},
    path::Path,
    process::Command,
};

use chrono::{DateTime, Local};
use serde::Deserialize;

#[derive(Deserialize)]
struct Project {
    host: String,
    path: String,
}

impl Project {
    #[allow(unused)]
    fn new<S>(host: S, path: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            host: host.into(),
            path: path.into(),
        }
    }

    /// Deserialize a [Project] from the TOML file at `path`.
    fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let toml = read_to_string(path)?;
        let res = toml::from_str(&toml)?;
        Ok(res)
    }
}

/// create a temporary directory and return its path
fn tempdir() -> io::Result<std::path::PathBuf> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let dir = base.join(format!("dash.{pid}"));
    create_dir_all(&dir)?;
    Ok(dir)
}

fn main() -> anyhow::Result<()> {
    let project = Project::load("test.toml")?;
    let temp = tempdir()?;
    let path =
        format!("{host}:{path}", host = project.host, path = project.path);
    let output = temp.join("path.dat");
    let mut cmd = Command::new("scp");
    cmd.arg("-p") // preserve mod times
        .arg(path)
        .arg(&output);
    if cmd.status().is_err() {
        eprintln!("failed to run command!");
    } else {
        let mut file = std::fs::File::open(output)?;
        let meta = file.metadata()?;
        let modified = meta.modified()?;
        let time: DateTime<Local> = DateTime::from(modified);
        println!("last modified at {time}");
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        println!("contents:\n{}", buf);
    }
    match remove_dir_all(temp) {
        Ok(_) => (),
        Err(e) => eprintln!("{e}"),
    }
    Ok(())
}
