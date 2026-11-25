use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::io::{BufRead, BufReader};
use which::which;
use crate::package_map::COLLECT_ALL_PACKAGES;

#[derive(Debug, Clone)]
pub enum BuildStep {
    Analyzing,
    CreatingVenv,
    InstallingDependencies,
    CompilingExe,
    CleaningUp,
    Completed(PathBuf),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct LogMessage {
    pub step: Option<BuildStep>,
    pub text: String,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub script_path: PathBuf,
    pub dependencies: Vec<String>,
    pub is_gui: bool,
    pub icon_path: Option<PathBuf>,
    pub additional_data: Vec<PathBuf>,
}

/// Найти лучший исполняемый файл Python в системе
pub fn find_python_executable() -> PathBuf {
    // 1. Проверяем в PATH
    if let Ok(path) = which("python") {
        return path;
    }
    if let Ok(path) = which("python3") {
        return path;
    }

    // 2. Стандартные пути Windows
    if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
        let py_bin = PathBuf::from(&local_app_data).join("Python").join("bin").join("python.exe");
        if py_bin.exists() {
            return py_bin;
        }
        let py_programs = PathBuf::from(&local_app_data).join("Programs").join("Python");
        if py_programs.exists() {
            if let Ok(entries) = fs::read_dir(&py_programs) {
                for entry in entries.flatten() {
                    let exe = entry.path().join("python.exe");
                    if exe.exists() {
                        return exe;
                    }
                }
            }
        }
    }

    PathBuf::from("python")
}

/// Проверить наличие `uv` в системе
pub fn is_uv_available() -> bool {
    which("uv").is_ok()
}

/// Выполнить команду и транслировать вывод построчно в канал
fn run_command_streaming(
    mut cmd: Command,
    tx: &Sender<LogMessage>,
    error_prefix: &str,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("{}: {}", error_prefix, e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let tx_out = tx.clone();
    let out_thread = std::thread::spawn(move || {
        if let Some(out) = stdout {
            let reader = BufReader::new(out);
            for line in reader.lines().flatten() {
                let _ = tx_out.send(LogMessage {
                    step: None,
                    text: line,
                    is_error: false,
                });
            }
        }
    });

    let tx_err = tx.clone();
    let err_thread = std::thread::spawn(move || {
        if let Some(err) = stderr {
            let reader = BufReader::new(err);
            for line in reader.lines().flatten() {
                let _ = tx_err.send(LogMessage {
                    step: None,
                    text: line,
                    is_error: false,
                });
            }
        }
    });

    let status = child.wait().map_err(|e| format!("Ошибка ожидания процесса: {}", e))?;
    let _ = out_thread.join();
    let _ = err_thread.join();

    if !status.success() {
        return Err(format!("Команда завершилась с кодом ошибки {:?}", status.code()));
    }
    Ok(())
}

/// Основной фоновый пайплайн сборки EXE
pub fn run_build_pipeline(options: BuildOptions, tx: Sender<LogMessage>) {
    let script_dir = match options.script_path.parent() {
        Some(d) => d.to_path_buf(),
        None => PathBuf::from("."),
    };
    let script_stem = match options.script_path.file_stem() {
        Some(s) => s.to_string_lossy().to_string(),
        None => "output".to_string(),
    };

    let python_exe = find_python_executable();
    let use_uv = is_uv_available();

    // 1. Создание временной директории для venv
    let temp_dir = match tempfile::Builder::new().prefix("py2exe_build_").tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            let _ = tx.send(LogMessage {
                step: Some(BuildStep::Failed(e.to_string())),
                text: format!("Ошибка создания временной папки: {}", e),
                is_error: true,
            });
            return;
        }
    };
    let venv_dir = temp_dir.path().join("venv");

    let _ = tx.send(LogMessage {
        step: Some(BuildStep::CreatingVenv),
        text: format!("Создание чистого venv: {}", venv_dir.display()),
        is_error: false,
    });

    let venv_python = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    };

    let create_venv_res = if use_uv {
        let _ = tx.send(LogMessage {
            step: None,
            text: "Используется быстрый 'uv venv'".to_string(),
            is_error: false,
        });
        let mut cmd = Command::new("uv");
        cmd.args(["venv", venv_dir.to_str().unwrap()]);
        run_command_streaming(cmd, &tx, "Сбой создания venv через uv")
    } else {
        let _ = tx.send(LogMessage {
            step: None,
            text: format!("Используется стандартный python -m venv ({})", python_exe.display()),
            is_error: false,
        });
        let mut cmd = Command::new(&python_exe);
        cmd.args(["-m", "venv", venv_dir.to_str().unwrap()]);
        run_command_streaming(cmd, &tx, "Сбой создания venv через python")
    };

    if let Err(e) = create_venv_res {
        let _ = tx.send(LogMessage {
            step: Some(BuildStep::Failed(e.clone())),
            text: format!("Не удалось создать изолированное окружение: {}", e),
            is_error: true,
        });
        return;
    }

    // 2. Установка зависимостей + PyInstaller
    let _ = tx.send(LogMessage {
        step: Some(BuildStep::InstallingDependencies),
        text: "Установка pyinstaller и зависимостей в изолированное окружение...".to_string(),
        is_error: false,
    });

    let mut pkgs_to_install = options.dependencies.clone();
    pkgs_to_install.push("pyinstaller".to_string());

    let install_res = if use_uv {
        let mut cmd = Command::new("uv");
        cmd.args(["pip", "install", "--python", venv_python.to_str().unwrap()]);
        for pkg in &pkgs_to_install {
            cmd.arg(pkg);
        }
        run_command_streaming(cmd, &tx, "Ошибка uv pip install")
    } else {
        let mut cmd = Command::new(&venv_python);
        cmd.args(["-m", "pip", "install", "--upgrade", "pip"]);
        let _ = run_command_streaming(cmd, &tx, "Обновление pip");

        let mut cmd = Command::new(&venv_python);
        cmd.args(["-m", "pip", "install"]);
        for pkg in &pkgs_to_install {
            cmd.arg(pkg);
        }
        run_command_streaming(cmd, &tx, "Ошибка pip install")
    };

    if let Err(e) = install_res {
        let _ = tx.send(LogMessage {
            step: None,
            text: format!("Внимание при установке пакетов: {}. Пробуем продолжить сборку...", e),
            is_error: true,
        });
    }

    // 3. Компиляция через PyInstaller
    let _ = tx.send(LogMessage {
        step: Some(BuildStep::CompilingExe),
        text: "Запуск PyInstaller...".to_string(),
        is_error: false,
    });

    let pyinstaller_exe = if cfg!(windows) {
        venv_dir.join("Scripts").join("pyinstaller.exe")
    } else {
        venv_dir.join("bin").join("pyinstaller")
    };

    let dist_dir = temp_dir.path().join("dist");
    let work_dir = temp_dir.path().join("build");
    let spec_dir = temp_dir.path().join("spec");

    let mut py_cmd = Command::new(&pyinstaller_exe);
    py_cmd.current_dir(&script_dir);
    py_cmd.args([
        "--noconfirm",
        "--clean",
        "--onefile",
        "--distpath", dist_dir.to_str().unwrap(),
        "--workpath", work_dir.to_str().unwrap(),
        "--specpath", spec_dir.to_str().unwrap(),
    ]);

    // Режим консоли или GUI
    if options.is_gui {
        py_cmd.arg("--noconsole");
        let _ = tx.send(LogMessage {
            step: None,
            text: "Добавлен флаг: --noconsole (GUI приложение)".to_string(),
            is_error: false,
        });
    } else {
        py_cmd.arg("--console");
        let _ = tx.send(LogMessage {
            step: None,
            text: "Режим: --console (консольное приложение)".to_string(),
            is_error: false,
        });
    }

    // Иконка
    if let Some(ref ico) = options.icon_path {
        if ico.exists() {
            py_cmd.arg(format!("--icon={}", ico.display()));
            let _ = tx.send(LogMessage {
                step: None,
                text: format!("Добавлена иконка: {}", ico.display()),
                is_error: false,
            });
        }
    }

    // Дополнительные данные (--add-data)
    for data in &options.additional_data {
        if data.exists() {
            let sep = if cfg!(windows) { ";" } else { ":" };
            let arg_val = format!("{}{}.", data.display(), sep);
            py_cmd.arg("--add-data");
            py_cmd.arg(&arg_val);
            let _ = tx.send(LogMessage {
                step: None,
                text: format!("Добавлен ресурс: {}", arg_val),
                is_error: false,
            });
        }
    }

    // Автоматический --collect-all для проблемных пакетов
    for dep in &options.dependencies {
        for &pkg in COLLECT_ALL_PACKAGES {
            if dep.eq_ignore_ascii_case(pkg) {
                py_cmd.arg("--collect-all");
                py_cmd.arg(pkg);
                let _ = tx.send(LogMessage {
                    step: None,
                    text: format!("Автоматически добавлен флаг --collect-all {}", pkg),
                    is_error: false,
                });
            }
        }
    }

    // Защита метаданных зависимостей (--copy-metadata) для предотвращения PackageNotFoundError
    for dep in &options.dependencies {
        py_cmd.arg("--copy-metadata");
        py_cmd.arg(dep);
        let _ = tx.send(LogMessage {
            step: None,
            text: format!("Добавлен флаг сохранения метаданных: --copy-metadata {}", dep),
            is_error: false,
        });
    }

    // Исходный скрипт
    py_cmd.arg(options.script_path.file_name().unwrap());

    if let Err(e) = run_command_streaming(py_cmd, &tx, "Ошибка выполнения PyInstaller") {
        let _ = tx.send(LogMessage {
            step: Some(BuildStep::Failed(e.clone())),
            text: format!("Ошибка компиляции: {}", e),
            is_error: true,
        });
        return;
    }

    // 4. Перемещение готового EXE к исходному скрипту
    let exe_name = if cfg!(windows) {
        format!("{}.exe", script_stem)
    } else {
        script_stem
    };
    let built_exe = dist_dir.join(&exe_name);
    let target_exe = script_dir.join(&exe_name);

    if built_exe.exists() {
        if let Err(e) = fs::copy(&built_exe, &target_exe) {
            let _ = tx.send(LogMessage {
                step: Some(BuildStep::Failed(e.to_string())),
                text: format!("Не удалось скопировать EXE в исходную папку: {}", e),
                is_error: true,
            });
            return;
        }
        let _ = tx.send(LogMessage {
            step: Some(BuildStep::CleaningUp),
            text: "Очистка временных файлов и виртуального окружения...".to_string(),
            is_error: false,
        });
    } else {
        let _ = tx.send(LogMessage {
            step: Some(BuildStep::Failed("Собранный .exe не найден в dist".to_string())),
            text: "PyInstaller завершился, но .exe файл не обнаружен".to_string(),
            is_error: true,
        });
        return;
    }

    // 5. Очистка временных файлов
    // TempDir удалится автоматически при выходе из области видимости,
    // но если PyInstaller создал файлы рядом с исходным скриптом (.spec), удаляем их
    let local_spec = script_dir.join(format!("{}.spec", options.script_path.file_stem().unwrap().to_string_lossy()));
    if local_spec.exists() {
        let _ = fs::remove_file(local_spec);
    }

    let _ = tx.send(LogMessage {
        step: Some(BuildStep::Completed(target_exe.clone())),
        text: format!("УСПЕШНО ЗАВЕРШЕНО! Исполняемый файл создан: {}", target_exe.display()),
        is_error: false,
    });
}
