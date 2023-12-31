use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use eframe::App;
use egui::{
    plot::{Line, Plot, PlotPoints},
    Color32, Window,
};

use crate::project::{default_interval, Config, Project, ProjectType};

pub(crate) struct MyApp {
    temp: PathBuf,
    config: Config,
    sender: Sender<(usize, PathBuf, Project)>,
    receiver: Receiver<(usize, Project)>,

    show_add: bool,
    show_add_name: String,
    show_add_host: String,
    show_add_path: String,
    show_add_type: String,

    /// vec of indices to delete from config.projects at the end of the plotting
    /// loop
    to_delete: Vec<usize>,
}

impl MyApp {
    pub(crate) fn new(temp: PathBuf, config: Config) -> Self {
        let (sender, inner_receiver) = channel::<(usize, PathBuf, Project)>();
        let (inner_sender, receiver) = channel();

        thread::spawn(move || {
            while let Ok((i, temp, mut project)) = inner_receiver.recv() {
                project.update(temp).unwrap();
                inner_sender.send((i, project)).unwrap();
            }
        });

        Self {
            config,
            sender,
            receiver,
            temp,
            show_add: false,
            show_add_name: String::new(),
            show_add_host: String::new(),
            show_add_path: String::new(),
            show_add_type: String::new(),
            to_delete: Vec::new(),
        }
    }

    /// Queue an update request for the project in `idx`
    fn request_update(&mut self, idx: usize) {
        // set this here so we don't keep queueing updates on the same
        // project
        self.config.projects[idx].last_updated = Instant::now();
        let p = &self.config.projects[idx];
        self.sender
            .send((idx, self.temp.clone(), p.clone()))
            .unwrap();
    }

    fn min_timeout(&self) -> u64 {
        self.config
            .projects
            .iter()
            .map(|p| p.update_interval)
            .min()
            .unwrap_or_else(default_interval)
    }

    fn add_project(&mut self, ctx: &egui::Context) {
        Window::new("Add a project")
            .default_size([200.0, 200.0])
            .show(ctx, |ui| {
                ui.label("name");
                ui.text_edit_singleline(&mut self.show_add_name);

                ui.label("host");
                ui.text_edit_singleline(&mut self.show_add_host);

                ui.label("path");
                ui.text_edit_singleline(&mut self.show_add_path);

                ui.label("type");
                ui.text_edit_singleline(&mut self.show_add_type);

                if ui.button("Add").clicked() {
                    let typ = match self.show_add_type.as_str() {
                        "pbqff" => ProjectType::Pbqff,
                        "semp" => ProjectType::Semp,
                        _ => panic!("invalid typ"),
                    };
                    self.config.projects.push(Project::new(
                        std::mem::take(&mut self.show_add_name),
                        std::mem::take(&mut self.show_add_host),
                        std::mem::take(&mut self.show_add_path),
                        typ,
                    ));
                    self.show_add_type.clear();
                    self.request_update(self.config.projects.len() - 1);
                }

                if ui.button("Close").clicked() {
                    self.show_add = false;
                }
            });
    }

    fn request_removal(&mut self, idx: usize) {
        self.to_delete.push(idx);
    }

    fn do_removal(&mut self) {
        // sort into descending order to remove from the end
        self.to_delete.sort_by(|a, b| b.cmp(a));
        let to_delete = std::mem::take(&mut self.to_delete);
        for i in to_delete {
            self.config.projects.remove(i);
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs(self.min_timeout()));
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Add Project").clicked() {
                        self.show_add = true;
                    }
                    if ui.button("Quit").clicked() {
                        frame.close();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let width = ui.available_width();
            let height = ui.available_height();
            if self.show_add {
                self.add_project(ctx);
            }

            for i in 0..self.config.projects.len() {
                if self.config.projects[i].needs_update() {
                    self.request_update(i);
                }

                let mut colors = [
                    Color32::RED,
                    Color32::GREEN,
                    Color32::from_rgb(0, 255, 255),
                    Color32::from_rgb(255, 0, 255),
                ]
                .into_iter()
                .cycle();

                Window::new(&self.config.projects[i].name)
                    .default_size([width / 2.0, height / 2.0])
                    .show(ctx, |ui| {
                        let project = &self.config.projects[i];
                        ui.label(format!(
                            "last updated {}",
                            project.last_modified
                        ));
                        let response = Plot::new(&project.path)
                            // TODO remove this when I get an answer
                            // https://github.com/emilk/egui/discussions/3101 and
                            // can handle zooming and right-clicking better
                            .allow_boxed_zoom(false)
                            .show(ui, |plot_ui| {
                                for ds in &project.data {
                                    plot_ui.line(
                                        Line::new(PlotPoints::new(
                                            ds.data.clone(),
                                        ))
                                        .color(colors.next().unwrap())
                                        .name(&ds.name),
                                    );
                                }
                            })
                            .response;

                        response.context_menu(|ui| {
                            if ui.button("Force Update").clicked() {
                                self.request_update(i);
                            }
                            if ui.button("SSH to Project").clicked() {
                                let path =
                                    Path::new(&self.config.projects[i].path);
                                let dir = path.parent().unwrap();
                                let mut cmd =
                                    Command::new(&self.config.terminal);
                                cmd.arg("-e")
                                    .arg("bash")
                                    .arg("-c")
                                    .arg(format!(
                                        "exec ssh -t {} 'cd {}; bash --login'",
                                        self.config.projects[i].host,
                                        dir.display()
                                    ))
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null());
                                cmd.spawn().unwrap();
                            }
                            if ui.button("Remove Project").clicked() {
                                self.request_removal(i);
                            }
                        });
                    });
            }

            self.do_removal();

            while let Ok((idx, project)) = self.receiver.try_recv() {
                self.config.projects[idx] = project;
            }
        });
    }
}
