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
    sender: Sender<(usize, PathBuf, Project)>,
    receiver: Receiver<(usize, Project)>,
}

impl MyApp {
    pub(crate) fn new(temp: PathBuf, projects: Vec<Project>) -> Self {
        let (sender, inner_receiver) =
            channel::<(usize, PathBuf, super::Project)>();
        let (inner_sender, receiver) = channel();

        thread::spawn(move || {
            while let Ok((i, temp, mut project)) = inner_receiver.recv() {
                project.update(temp).unwrap();
                inner_sender.send((i, project)).unwrap();
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
            let name = match project.typ {
                ProjectType::Semp => "RMSD",
                ProjectType::Pbqff => "jobs remaining",
            };
            Window::new(&project.name)
                .default_size([400.0, 400.0])
                .show(ctx, |ui| {
                    Plot::new(&project.path).show(ui, |plot_ui| {
                        plot_ui.line(
                            Line::new(PlotPoints::new(project.data.clone()))
                                .color(Color32::from_rgb(200, 100, 100))
                                .name(name),
                        );
                    });
                });
        }

        while let Ok((idx, project)) = self.receiver.try_recv() {
            self.projects[idx] = project;
        }
    }
}
