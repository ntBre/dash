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

impl Project {
    /// Deserialize a [Project] from the TOML file at `path`.
    fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let toml = read_to_string(path)?;
        let res = toml::from_str(&toml)?;
        Ok(res)
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
}

mod gui {
    use std::{
        path::PathBuf,
        sync::mpsc::{channel, Receiver, Sender},
        thread,
        time::Instant,
    };

    use eframe::App;
    use egui::{
        plot::{Line, Plot, PlotPoints},
        Color32, Window,
    };

    use crate::Project;

    pub(crate) struct MyApp {
        temp: PathBuf,
        projects: Vec<Project>,
        sender: Sender<(usize, PathBuf, super::Project)>,
        receiver: Receiver<(usize, Vec<[f64; 2]>)>,
    }

    impl MyApp {
        pub(crate) fn new(temp: PathBuf, projects: Vec<Project>) -> Self {
            let (sender, inner_receiver) =
                channel::<(usize, PathBuf, super::Project)>();
            let (inner_sender, receiver) = channel();

            thread::spawn(move || {
                while let Ok((i, temp, project)) = inner_receiver.recv() {
                    let fetch = project.fetch(temp).unwrap();
                    let out = fetch.parse();
                    inner_sender.send((i, out)).unwrap();
                }
            });

            Self {
                projects,
                sender,
                receiver,
                temp,
            }
        }

        /// Queue an update request for the project in `idx`
        fn request_update(&mut self, idx: usize) {
            // set this here so we don't keep queueing updates on the same
            // project
            self.projects[idx].last_updated = Instant::now();
            let p = &self.projects[idx];
            self.sender
                .send((idx, self.temp.clone(), p.clone()))
                .unwrap();
        }
    }

    impl App for MyApp {
        fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            frame.close();
                        }
                    });
                });
            });

            for i in 0..self.projects.len() {
                if self.projects[i].needs_update() {
                    self.request_update(i);
                }

                let project = &self.projects[i];
                Window::new(&project.name)
                    .default_size([400.0, 400.0])
                    .show(ctx, |ui| {
                        Plot::new(&project.path).show(ui, |plot_ui| {
                            plot_ui.line(
                                Line::new(PlotPoints::new(
                                    project.data.clone(),
                                ))
                                .color(Color32::from_rgb(200, 100, 100))
                                .name("wave"),
                            );
                        });
                    });
            }

            while let Ok((idx, data)) = self.receiver.try_recv() {
                let p = &mut self.projects[idx];
                p.data = data;
                p.last_updated = Instant::now();
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let temp = tempdir()?;

    let mut project = Project::load("test.toml")?;
    let fetch = project.fetch(&temp)?;
    project.data = fetch.parse();

    let app = MyApp::new(temp.clone(), vec![project]);

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
