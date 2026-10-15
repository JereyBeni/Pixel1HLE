//! Host file explorer – browse drives / folders and pick .apk files

use egui::{self, Color32, RichText, Sense, Vec2};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileExplorer {
    pub cwd: PathBuf,
    pub entries: Vec<DirEntryView>,
    pub error: Option<String>,
    pub selected: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DirEntryView {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_apk: bool,
    pub size_mb: f32,
}

impl Default for FileExplorer {
    fn default() -> Self {
        let cwd = default_start_dir();
        let mut fe = Self {
            cwd: cwd.clone(),
            entries: Vec::new(),
            error: None,
            selected: None,
        };
        fe.refresh();
        fe
    }
}

impl FileExplorer {
    pub fn refresh(&mut self) {
        self.error = None;
        self.entries.clear();
        match std::fs::read_dir(&self.cwd) {
            Ok(rd) => {
                let mut dirs = Vec::new();
                let mut files = Vec::new();
                for e in rd.flatten() {
                    let path = e.path();
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let meta = e.metadata().ok();
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let is_apk = !is_dir
                        && path
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| s.eq_ignore_ascii_case("apk"))
                            .unwrap_or(false);
                    let size_mb = meta
                        .map(|m| m.len() as f32 / 1_048_576.0)
                        .unwrap_or(0.0);
                    let view = DirEntryView {
                        name,
                        path,
                        is_dir,
                        is_apk,
                        size_mb,
                    };
                    if is_dir {
                        dirs.push(view);
                    } else {
                        files.push(view);
                    }
                }
                dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.entries = dirs;
                self.entries.extend(files);
            }
            Err(e) => {
                self.error = Some(format!("Cannot read {}: {e}", self.cwd.display()));
            }
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            self.refresh();
        }
    }

    pub fn enter(&mut self, path: &Path) {
        if path.is_dir() {
            self.cwd = path.to_path_buf();
            self.refresh();
        }
    }

    /// Draw explorer inside a panel; returns Some(apk_path) when user confirms.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let mut picked = None;

        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(RichText::new("↑ Up").color(Color32::WHITE)).fill(
                    Color32::from_rgb(40, 48, 60),
                ))
                .clicked()
            {
                self.go_up();
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("↻ Refresh").color(Color32::WHITE))
                        .fill(Color32::from_rgb(40, 48, 60)),
                )
                .clicked()
            {
                self.refresh();
            }
            #[cfg(windows)]
            {
                for letter in ["C:\\", "D:\\", "E:\\"] {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(letter).small().color(Color32::LIGHT_BLUE))
                                .fill(Color32::from_rgb(30, 36, 48)),
                        )
                        .clicked()
                    {
                        self.cwd = PathBuf::from(letter);
                        self.refresh();
                    }
                }
            }
            #[cfg(target_os = "android")]
            {
                for root in ["/sdcard", "/sdcard/Download", "/storage/emulated/0"] {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(root).small().color(Color32::LIGHT_BLUE))
                                .fill(Color32::from_rgb(30, 36, 48)),
                        )
                        .clicked()
                    {
                        self.cwd = PathBuf::from(root);
                        self.refresh();
                    }
                }
            }
        });

        ui.label(
            RichText::new(self.cwd.to_string_lossy())
                .size(12.0)
                .color(Color32::from_rgb(140, 160, 180)),
        );

        if let Some(err) = &self.error {
            ui.colored_label(Color32::from_rgb(220, 80, 80), err);
        }

        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 48.0)
            .show(ui, |ui| {
                let entries = self.entries.clone();
                for entry in &entries {
                    let icon = if entry.is_dir {
                        "📁"
                    } else if entry.is_apk {
                        "📦"
                    } else {
                        "📄"
                    };
                    let label = if entry.is_apk {
                        format!("{}  {}  ({:.1} MB)", icon, entry.name, entry.size_mb)
                    } else {
                        format!("{}  {}", icon, entry.name)
                    };
                    let color = if entry.is_apk {
                        Color32::from_rgb(100, 220, 140)
                    } else if entry.is_dir {
                        Color32::from_rgb(180, 200, 220)
                    } else {
                        Color32::from_rgb(120, 130, 140)
                    };

                    let resp = ui.add(
                        egui::Button::new(RichText::new(label).color(color).size(13.0))
                            .fill(Color32::from_rgb(24, 28, 36))
                            .min_size(Vec2::new(ui.available_width(), 26.0)),
                    );

                    if resp.clicked() {
                        if entry.is_dir {
                            self.enter(&entry.path);
                        } else if entry.is_apk {
                            self.selected = Some(entry.path.clone());
                        }
                    }
                    if resp.double_clicked() && entry.is_apk {
                        picked = Some(entry.path.clone());
                    }
                }
            });

        ui.horizontal(|ui| {
            if let Some(sel) = &self.selected {
                ui.label(
                    RichText::new(format!(
                        "Selected: {}",
                        sel.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ))
                    .color(Color32::from_rgb(80, 220, 120)),
                );
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Import ya!")
                                .strong()
                                .color(Color32::from_rgb(20, 20, 28)),
                        )
                        .fill(Color32::from_rgb(80, 220, 120)),
                    )
                    .clicked()
                {
                    picked = Some(sel.clone());
                }
            } else {
                ui.label(
                    RichText::new("Select an .apk or double-click to import")
                        .color(Color32::from_rgb(100, 110, 130)),
                );
            }
        });

        picked
    }
}

fn default_start_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(ud) = std::env::var_os("USERPROFILE") {
            let dl = PathBuf::from(&ud).join("Downloads");
            if dl.is_dir() {
                return dl;
            }
            return PathBuf::from(ud);
        }
        PathBuf::from("C:\\")
    }
    #[cfg(target_os = "android")]
    {
        for p in ["/sdcard/Download", "/storage/emulated/0/Download", "/sdcard"] {
            let pb = PathBuf::from(p);
            if pb.is_dir() {
                return pb;
            }
        }
        PathBuf::from("/")
    }
    #[cfg(all(unix, not(target_os = "android")))]
    {
        if let Some(h) = std::env::var_os("HOME") {
            let dl = PathBuf::from(&h).join("Downloads");
            if dl.is_dir() {
                return dl;
            }
            return PathBuf::from(h);
        }
        PathBuf::from("/")
    }
}
