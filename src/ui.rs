use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use rfd::FileDialog;

use crate::analyzer::{analyze_python_script, AnalysisResult};
use crate::builder::{find_python_executable, run_build_pipeline, BuildOptions, BuildStep, LogMessage};

pub struct Py2ExeApp {
    // Выбранный файл и опции
    pub script_path: Option<PathBuf>,
    pub dependencies: Vec<String>,
    pub new_dep_input: String,
    pub is_gui: bool,
    pub icon_path: Option<PathBuf>,
    pub additional_data: Vec<PathBuf>,
    pub auto_build_on_drop: bool,

    // Анализ
    pub analysis_result: Option<AnalysisResult>,
    pub is_analyzing: bool,

    // Процесс сборки
    pub is_building: bool,
    pub current_step: Option<BuildStep>,
    pub log_messages: Vec<LogMessage>,
    pub completed_exe: Option<PathBuf>,
    pub error_message: Option<String>,

    // Канал общения с фоновым потоком
    rx_build: Option<Receiver<LogMessage>>,
    tx_build: Sender<LogMessage>,
}

impl Default for Py2ExeApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            script_path: None,
            dependencies: Vec::new(),
            new_dep_input: String::new(),
            is_gui: false,
            icon_path: None,
            additional_data: Vec::new(),
            auto_build_on_drop: true,

            analysis_result: None,
            is_analyzing: false,

            is_building: false,
            current_step: None,
            log_messages: Vec::new(),
            completed_exe: None,
            error_message: None,

            rx_build: Some(rx),
            tx_build: tx,
        }
    }
}

impl Py2ExeApp {
    pub fn select_and_process_file(&mut self, path: PathBuf) {
        if path.extension().and_then(|s| s.to_str()) != Some("py") {
            self.error_message = Some("Пожалуйста, выберите файл с расширением .py".to_string());
            return;
        }

        self.script_path = Some(path.clone());
        self.error_message = None;
        self.completed_exe = None;
        self.log_messages.clear();

        // Запуск анализа
        self.is_analyzing = true;
        let python_path = find_python_executable();
        match analyze_python_script(&path, &python_path) {
            Ok(res) => {
                self.dependencies = res.external_packages.clone();
                self.is_gui = res.is_gui;
                self.analysis_result = Some(res);
                self.is_analyzing = false;

                // Если включен автозапуск сборки при перетаскивании
                if self.auto_build_on_drop {
                    self.start_build();
                }
            }
            Err(e) => {
                self.is_analyzing = false;
                self.error_message = Some(format!("Ошибка анализа: {}", e));
            }
        }
    }

    pub fn start_build(&mut self) {
        if self.is_building {
            return;
        }
        let Some(script_path) = self.script_path.clone() else {
            self.error_message = Some("Сначала выберите .py файл".to_string());
            return;
        };

        self.is_building = true;
        self.completed_exe = None;
        self.error_message = None;
        self.log_messages.clear();
        self.current_step = Some(BuildStep::Analyzing);

        let (tx, rx) = channel();
        self.rx_build = Some(rx);
        self.tx_build = tx.clone();

        let options = BuildOptions {
            script_path,
            dependencies: self.dependencies.clone(),
            is_gui: self.is_gui,
            icon_path: self.icon_path.clone(),
            additional_data: self.additional_data.clone(),
        };

        thread::spawn(move || {
            run_build_pipeline(options, tx);
        });
    }

    fn check_build_progress(&mut self) {
        if let Some(ref rx) = self.rx_build {
            while let Ok(msg) = rx.try_recv() {
                if let Some(step) = &msg.step {
                    self.current_step = Some(step.clone());
                    match step {
                        BuildStep::Completed(exe_path) => {
                            self.is_building = false;
                            self.completed_exe = Some(exe_path.clone());
                        }
                        BuildStep::Failed(err) => {
                            self.is_building = false;
                            self.error_message = Some(err.clone());
                        }
                        _ => {}
                    }
                }
                self.log_messages.push(msg);
            }
        }
    }
}

impl eframe::App for Py2ExeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_build_progress();

        // Запрос постоянной перерисовки во время сборки для плавности логов
        if self.is_building {
            ctx.request_repaint();
        }

        // Обработка Drag & Drop на всем окне
        if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if let Some(first_file) = dropped.first() {
                if let Some(ref path) = first_file.path {
                    self.select_and_process_file(path.clone());
                }
            }
        }

        // Верхняя шапка
        egui::TopBottomPanel::top("header_panel").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("⚡ Halium AutoCompiler").color(Color32::from_rgb(90, 190, 255)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.auto_build_on_drop, "Автосборка при Drop");
                });
            });
            ui.add_space(4.0);
        });

        // Центральная панель
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // 1. Drag & Drop Зона
                let drop_zone_height = 100.0;
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), drop_zone_height),
                    egui::Sense::click(),
                );

                let is_hovered = response.hovered();
                let bg_color = if is_hovered {
                    Color32::from_rgb(35, 45, 60)
                } else {
                    Color32::from_rgb(28, 30, 36)
                };
                let stroke_color = if is_hovered {
                    Color32::from_rgb(90, 190, 255)
                } else {
                    Color32::from_rgb(70, 75, 90)
                };

                ui.painter().rect(
                    rect,
                    8.0,
                    bg_color,
                    Stroke::new(1.5_f32, stroke_color),
                );

                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(18.0);
                        ui.label(RichText::new("📥 Перетащите сюда ваш .py файл").size(16.0).strong());
                        ui.add_space(4.0);
                        if ui.button("📁 Или выберите файл через диалог...").clicked() {
                            if let Some(file) = FileDialog::new()
                                .add_filter("Python Script", &["py"])
                                .pick_file()
                            {
                                self.select_and_process_file(file);
                            }
                        }
                    });
                });

                ui.add_space(10.0);

                // Отображение выбранного файла
                if let Some(ref path) = self.script_path {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Выбранный скрипт:").strong());
                            ui.label(RichText::new(path.to_string_lossy()).color(Color32::from_rgb(120, 220, 140)));
                        });
                    });
                }

                // 2. Индикатор шагов (Progress Indicator)
                if self.is_building || self.completed_exe.is_some() || self.error_message.is_some() {
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let steps = [
                                ("1. Анализ", 0),
                                ("2. Создание venv", 1),
                                ("3. Зависимости", 2),
                                ("4. Компиляция", 3),
                                ("5. Готово", 4),
                            ];

                            let current_idx = match &self.current_step {
                                Some(BuildStep::Analyzing) => 0,
                                Some(BuildStep::CreatingVenv) => 1,
                                Some(BuildStep::InstallingDependencies) => 2,
                                Some(BuildStep::CompilingExe) => 3,
                                Some(BuildStep::CleaningUp) | Some(BuildStep::Completed(_)) => 4,
                                _ => -1,
                            };

                            for (title, idx) in steps {
                                let (color, text_style) = if current_idx == idx && self.is_building {
                                    (Color32::from_rgb(90, 190, 255), true)
                                } else if current_idx > idx || self.completed_exe.is_some() {
                                    (Color32::from_rgb(100, 220, 100), false)
                                } else {
                                    (Color32::from_rgb(110, 110, 120), false)
                                };

                                let rich = if text_style {
                                    RichText::new(title).color(color).strong()
                                } else {
                                    RichText::new(title).color(color)
                                };
                                ui.label(rich);
                                if idx < 4 {
                                    ui.label("➔");
                                }
                            }
                        });
                    });
                }

                // Успешный результат
                if let Some(ref exe_path) = self.completed_exe {
                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🎉 Сборка завершена:").color(Color32::from_rgb(80, 230, 120)).strong());
                            ui.label(exe_path.file_name().unwrap().to_string_lossy());
                            if ui.button("📂 Открыть папку с EXE").clicked() {
                                if let Some(parent) = exe_path.parent() {
                                    let _ = std::process::Command::new("explorer").arg(parent).spawn();
                                }
                            }
                        });
                    });
                }

                // Ошибка
                if let Some(ref err) = self.error_message {
                    ui.add_space(8.0);
                    ui.colored_label(Color32::from_rgb(255, 90, 90), format!("❌ Ошибка: {}", err));
                }

                ui.add_space(10.0);

                // 3. Настройки и обнаруженные зависимости
                ui.collapsing("⚙ Параметры сборки и зависимости", |ui| {
                    ui.checkbox(&mut self.is_gui, "Режим оконного приложения (--noconsole / GUI)");

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Иконка (.ico):");
                        if let Some(ref ico) = self.icon_path {
                            ui.label(ico.file_name().unwrap().to_string_lossy());
                            if ui.button("✖").clicked() {
                                self.icon_path = None;
                            }
                        } else if ui.button("Выбрать иконку...").clicked() {
                            if let Some(ico) = FileDialog::new().add_filter("Icon", &["ico"]).pick_file() {
                                self.icon_path = Some(ico);
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.label("Дополнительные ресурсы (--add-data):");
                    for (i, data) in self.additional_data.clone().iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("• {}", data.display()));
                            if ui.button("Удалить").clicked() {
                                self.additional_data.remove(i);
                            }
                        });
                    }
                    if ui.button("+ Добавить файл/папку ресурсов").clicked() {
                        if let Some(file) = FileDialog::new().pick_file() {
                            self.additional_data.push(file);
                        }
                    }

                    ui.add_space(8.0);
                    ui.label(RichText::new("Найденные зависимости PyPI:").strong());

                    // Теги зависимостей
                    ui.horizontal_wrapped(|ui| {
                        let mut to_remove = None;
                        for (idx, dep) in self.dependencies.iter().enumerate() {
                            let tag_text = format!("📦 {} ✖", dep);
                            if ui.button(RichText::new(tag_text).color(Color32::from_rgb(180, 210, 255))).clicked() {
                                to_remove = Some(idx);
                            }
                        }
                        if let Some(idx) = to_remove {
                            self.dependencies.remove(idx);
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.new_dep_input);
                        if ui.button("+ Добавить пакет").clicked() {
                            let trimmed = self.new_dep_input.trim();
                            if !trimmed.is_empty() && !self.dependencies.contains(&trimmed.to_string()) {
                                self.dependencies.push(trimmed.to_string());
                                self.new_dep_input.clear();
                            }
                        }
                    });
                });

                ui.add_space(10.0);

                // Кнопка ручного запуска сборки
                let can_build = self.script_path.is_some() && !self.is_building;
                let btn_text = if self.is_building {
                    "⏳ Идет компиляция..."
                } else {
                    "🚀 Скомпилировать в .EXE"
                };

                ui.add_enabled_ui(can_build, |ui| {
                    if ui.add_sized([ui.available_width(), 40.0], egui::Button::new(RichText::new(btn_text).size(16.0).strong())).clicked() {
                        self.start_build();
                    }
                });

                ui.add_space(10.0);

                // 4. Сворачиваемый живой лог терминала
                ui.collapsing("📜 Лог выполнения сборки", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for log in &self.log_messages {
                                let color = if log.is_error {
                                    Color32::from_rgb(255, 110, 110)
                                } else {
                                    Color32::from_rgb(210, 215, 220)
                                };
                                ui.colored_label(color, &log.text);
                            }
                        });
                });
            });
        });
    }
}
