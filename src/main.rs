use std::{
    fs::{create_dir_all, read_to_string, remove_dir_all},
    io::{self, Read},
    path::Path,
    process::Command,
    time::Instant,
};

use chrono::{DateTime, Local};
use gui::MyApp;
use serde::Deserialize;

mod gui;

/// create a temporary directory and return its path
fn tempdir() -> io::Result<std::path::PathBuf> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let dir = base.join(format!("dash.{pid}"));
    create_dir_all(&dir)?;
    Ok(dir)
}

const fn default_interval() -> u64 {
    60
}

#[derive(Clone, Deserialize)]
struct Project {
    name: String,
    host: String,
    path: String,

    #[serde(default = "Instant::now")]
    #[serde(skip_deserializing)]
    last_updated: Instant,

    /// update interval in seconds
    #[serde(default = "default_interval")]
    update_interval: u64,

    #[serde(default)]
    #[serde(skip_deserializing)]
    data: Vec<[f64; 2]>,
}

struct Fetch {
    #[allow(unused)]
    last_modified: DateTime<Local>,
    contents: String,
}

impl Fetch {
    /// parse `self` into a sequence of data points for plotting by egui
    fn parse(&self) -> Vec<[f64; 2]> {
        let mut i = 0;
        let mut ret = Vec::new();
        for line in self.contents.lines() {
            let mut sp = line.split_ascii_whitespace();
            if sp.next().is_some_and(|s| s.chars().all(|c| c.is_numeric())) {
                let s = sp.next().unwrap().parse().unwrap();
                ret.push([i as f64, s]);
                i += 1;
            }
        }
        ret
    }
}

#[derive(Deserialize)]
struct Projects {
    project: Vec<Project>,
}

impl Project {
    /// Deserialize a set of [Project]s from the TOML file at `path`, and update
    /// them using [Project::update].
    fn load(
        path: impl AsRef<Path>,
        temp: impl AsRef<Path>,
    ) -> anyhow::Result<Vec<Self>> {
        let toml = read_to_string(path)?;
        let mut projects: Projects = toml::from_str(&toml)?;

        for p in projects.project.iter_mut() {
            p.update(&temp)?;
        }

        Ok(projects.project)
    }

    /// Retrieve the remote files for `self`, storing temporary files in `temp`.
    /// Returns a [Fetch] containing the resulting data.
    fn fetch(&self, temp: impl AsRef<Path>) -> anyhow::Result<Fetch> {
        let path = format!("{host}:{path}", host = self.host, path = self.path);
        let output = temp.as_ref().join("path.dat");
        eprintln!("calling fetch on {path} at {}", Local::now());
        let mut cmd = Command::new("scp");
        cmd.arg("-p") // preserve mod times
            .arg(path)
            .arg(&output);
        cmd.status()?;
        let mut file = std::fs::File::open(output)?;
        let meta = file.metadata()?;
        let modified = meta.modified()?;
        let last_modified: DateTime<Local> = DateTime::from(modified);
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        Ok(Fetch {
            last_modified,
            contents,
        })
    }

    fn needs_update(&self) -> bool {
        let now = Instant::now();
        now.duration_since(self.last_updated).as_secs() > self.update_interval
    }

    fn update(&mut self, temp: impl AsRef<Path>) -> anyhow::Result<()> {
        let fetch = self.fetch(temp)?;
        let data = fetch.parse();
        self.data = data;
        self.last_updated = Instant::now();
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let temp = tempdir()?;

    let projects = Project::load("test.toml", &temp)?;

    let app = MyApp::new(temp.clone(), projects);

    const PROGRAM_TITLE: &str = "dash";
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
