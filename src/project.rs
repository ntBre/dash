use std::fs::read_to_string;
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use chrono::DateTime;
use chrono::Local;
use serde::Deserialize;

use super::DEBUG;

pub(crate) const fn default_interval() -> u64 {
    60
}

#[derive(Clone, Copy, Deserialize)]
pub(crate) enum ProjectType {
    #[serde(alias = "semp")]
    Semp,
    #[serde(alias = "pbqff")]
    Pbqff,
}

#[derive(Clone, Deserialize)]
pub(crate) struct Project {
    pub(crate) name: String,
    pub(crate) host: String,
    pub(crate) path: String,

    #[serde(alias = "type")]
    pub(crate) typ: ProjectType,

    #[serde(default = "Instant::now")]
    #[serde(skip_deserializing)]
    pub(crate) last_updated: Instant,

    /// update interval in seconds
    #[serde(default = "default_interval")]
    pub(crate) update_interval: u64,

    #[serde(default)]
    #[serde(skip_deserializing)]
    pub(crate) data: Vec<[f64; 2]>,
}

pub(crate) struct Fetch {
    #[allow(unused)]
    pub(crate) last_modified: DateTime<Local>,
    pub(crate) contents: String,
}

impl Fetch {
    /// parse `self` into a sequence of data points for plotting by egui
    pub(crate) fn parse(&self, typ: ProjectType) -> Vec<[f64; 2]> {
        match typ {
            ProjectType::Semp => self.parse_semp(),
            ProjectType::Pbqff => self.parse_pbqff(),
        }
    }

    pub(crate) fn parse_semp(&self) -> Vec<[f64; 2]> {
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

    pub(crate) fn parse_pbqff(&self) -> Vec<[f64; 2]> {
        let mut ret = Vec::new();
        let mut did_drop = false;
        for line in self.contents.lines() {
            if line.starts_with("finished dropping") {
                did_drop = true;
            }
            if line.starts_with("[iter ") {
                // only track the current phase of the QFF. if we dropped and
                // found more [iter ...] lines, we've entered a new phase
                if did_drop {
                    did_drop = false;
                    ret.clear();
                }
                let sp: Vec<_> = line.split_ascii_whitespace().collect();
                let i = sp[1].parse().unwrap();
                let remaining = sp[7].parse().unwrap();
                ret.push([i, remaining]);
            }
        }
        ret
    }
}

#[derive(Deserialize)]
pub(crate) struct Projects {
    pub(crate) project: Vec<Project>,
}

impl Project {
    /// Deserialize a set of [Project]s from the TOML file at `path`, and update
    /// them using [Project::update].
    pub(crate) fn load(
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
    pub(crate) fn fetch(
        &self,
        temp: impl AsRef<Path>,
    ) -> anyhow::Result<Fetch> {
        let path = format!("{host}:{path}", host = self.host, path = self.path);
        let output = temp.as_ref().join("path.dat");
        if *DEBUG {
            eprintln!("calling fetch on {path} at {}", Local::now());
        }
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

    pub(crate) fn needs_update(&self) -> bool {
        let now = Instant::now();
        now.duration_since(self.last_updated).as_secs() > self.update_interval
    }

    pub(crate) fn update(
        &mut self,
        temp: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        let fetch = self.fetch(temp)?;
        let data = fetch.parse(self.typ);
        self.data = data;
        self.last_updated = Instant::now();
        Ok(())
    }
}
