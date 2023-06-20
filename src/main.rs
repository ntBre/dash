use std::{
    fs::{create_dir_all, read_to_string, remove_dir_all},
    io::{self, Read},
    path::Path,
    process::Command,
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

#[derive(Deserialize)]
struct Project {
    host: String,
    path: String,
}

struct Fetch {
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

    /// Retrieve the remote files for `self`, storing temporary files in `temp`.
    /// Returns a [Fetch] containing the resulting data.
    fn fetch(&self, temp: impl AsRef<Path>) -> anyhow::Result<Fetch> {
        let path = format!("{host}:{path}", host = self.host, path = self.path);
        let output = temp.as_ref().join("path.dat");
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
}

mod gui {
    use eframe::App;
    use egui::{
        plot::{Line, Plot, PlotPoints},
        Color32, Window,
    };

    pub(crate) struct MyApp {
        data: Vec<[f64; 2]>,
    }

    impl MyApp {
        pub(crate) fn new(data: Vec<[f64; 2]>) -> Self {
            Self { data }
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

            Window::new("plot window")
                .default_size([400.0, 400.0])
                .show(ctx, |ui| {
                    Plot::new("job progress").show(ui, |plot_ui| {
                        plot_ui.line(
                            Line::new(PlotPoints::new(self.data.clone()))
                                .color(Color32::from_rgb(200, 100, 100))
                                .name("wave"),
                        );
                    });
                });
        }
    }
}

fn main() -> anyhow::Result<()> {
    let project = Project::load("test.toml")?;
    let temp = tempdir()?;

    let fetch = project.fetch(&temp)?;

    println!("last modified at {m}", m = fetch.last_modified);
    println!("contents:\n{c}", c = fetch.contents);

    let app = MyApp::new(fetch.parse());

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
