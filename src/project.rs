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
    600
}

fn default_terminal() -> String {
    "st".to_string()
}

#[derive(Clone, Copy, Deserialize)]
pub(crate) enum ProjectType {
    #[serde(alias = "semp")]
    Semp,
    #[serde(alias = "pbqff")]
    Pbqff,
}

#[derive(Clone, Default, Deserialize)]
pub(crate) struct DataSet {
    pub(crate) name: String,
    pub(crate) data: Vec<[f64; 2]>,
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
    pub(crate) data: Vec<DataSet>,

    #[serde(default)]
    #[serde(skip_deserializing)]
    pub(crate) last_modified: DateTime<Local>,
}

pub(crate) struct Fetch {
    pub(crate) last_modified: DateTime<Local>,
    pub(crate) data: Vec<DataSet>,
}

/// parse a semp freqs.log file
pub(crate) fn parse_freqs(contents: String) -> DataSet {
    let mut mae = DataSet {
        name: "MAE".to_owned(),
        data: Vec::new(),
    };
    for line in contents.lines() {
        let sp: Vec<_> = line.split_ascii_whitespace().collect();
        mae.data.push([
            sp[0].parse().unwrap(),
            sp.last().unwrap().parse().unwrap(),
        ]);
    }
    mae
}

/// parse a semp output file
pub(crate) fn parse_semp(contents: String) -> Vec<DataSet> {
    let mut i = 0;
    let mut norm = DataSet {
        name: "Norm".to_owned(),
        data: Vec::new(),
    };
    let mut rmsd = DataSet {
        name: "RMSD".to_owned(),
        data: Vec::new(),
    };
    let mut max = DataSet {
        name: "MAX".to_owned(),
        data: Vec::new(),
    };
    for line in contents.lines() {
        let mut sp = line.split_ascii_whitespace();
        if sp.next().is_some_and(|s| s.chars().all(|c| c.is_numeric())) {
            let n = sp.next().unwrap().parse().unwrap();
            norm.data.push([i as f64, n]);
            let r = sp.nth(1).unwrap().parse().unwrap();
            rmsd.data.push([i as f64, r]);
            let m = sp.nth(1).unwrap().parse().unwrap();
            max.data.push([i as f64, m]);
            i += 1;
        }
    }
    vec![norm, rmsd, max]
}

pub(crate) fn parse_pbqff(contents: String) -> Vec<DataSet> {
    let mut ret = DataSet {
        name: "Points remaining".to_owned(),
        data: Vec::new(),
    };
    let mut did_drop = false;
    for line in contents.lines() {
        if line.starts_with("finished dropping") {
            did_drop = true;
        }
        if line.starts_with("[iter ") {
            // only track the current phase of the QFF. if we dropped and
            // found more [iter ...] lines, we've entered a new phase
            if did_drop {
                did_drop = false;
                ret.data.clear();
            }
            let sp: Vec<_> = line.split_ascii_whitespace().collect();
            let i = sp[1].parse().unwrap();
            let remaining = sp[7].parse().unwrap();
            ret.data.push([i, remaining]);
        }
    }
    vec![ret]
}

#[derive(Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    #[serde(rename = "project")]
    pub(crate) projects: Vec<Project>,

    #[serde(default = "default_terminal")]
    pub(crate) terminal: String,
}

impl Config {
    /// Deserialize a set of [Project]s from the TOML file at `path`, and update
    /// them using [Project::update].
    pub(crate) fn load(
        path: impl AsRef<Path>,
        temp: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let toml = read_to_string(path)?;
        let mut projects: Config = toml::from_str(&toml)?;

        for p in projects.projects.iter_mut() {
            p.update(&temp)?;
        }

        Ok(projects)
    }
}

impl Project {
    pub(crate) fn new(
        name: String,
        host: String,
        path: String,
        typ: ProjectType,
    ) -> Self {
        Self {
            name,
            host,
            path,
            typ,
            last_updated: Instant::now(),
            update_interval: default_interval(),
            data: Vec::new(),
            last_modified: Default::default(),
        }
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
            .arg("-C") // use compression
            .arg(path)
            .arg(&output);
        cmd.status()?;
        let mut file = std::fs::File::open(output)?;
        let meta = file.metadata()?;
        let modified = meta.modified()?;
        let last_modified: DateTime<Local> = DateTime::from(modified);
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let data = match self.typ {
            ProjectType::Semp => {
                let mut data = parse_semp(contents);
                // this will be the path of the semp.out file, so get the parent
                // directory and then re-join freqs.log
                let path = Path::new(&self.path);
                let freqs = path.parent().unwrap().join("freqs.log");
                let path = format!(
                    "{host}:{path}",
                    host = self.host,
                    path = freqs.display()
                );
                let output = temp.as_ref().join("freqs.log");
                let mut cmd = Command::new("scp");
                cmd.arg("-p") // preserve mod times
                    .arg("-C") // use compression
                    .arg(path)
                    .arg(&output);
                cmd.status()?;
                let mut file = std::fs::File::open(output)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                data.push(parse_freqs(contents));
                data
            }
            ProjectType::Pbqff => parse_pbqff(contents),
        };

        Ok(Fetch {
            last_modified,
            data,
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
        let Fetch {
            last_modified,
            data,
        } = self.fetch(temp)?;
        self.data = data;
        self.last_updated = Instant::now();
        self.last_modified = last_modified;
        Ok(())
    }
}
