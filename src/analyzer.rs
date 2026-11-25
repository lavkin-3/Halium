use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use regex::Regex;
use crate::package_map::{get_package_map, GUI_MODULES};

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Внешние зависимости (PyPI пакеты)
    pub external_packages: Vec<String>,
    /// Все найденные модули верхнего уровня (включая локальные)
    #[allow(dead_code)]
    pub detected_modules: Vec<String>,
    /// Обнаружено ли использование GUI фреймворков
    pub is_gui: bool,
    /// Проверенные локальные py файлы
    #[allow(dead_code)]
    pub local_files: Vec<PathBuf>,
}

/// Получение списка модулей стандартной библиотеки Python.
pub fn get_stdlib_modules(python_path: &Path) -> HashSet<String> {
    let mut stdlib = HashSet::new();

    // Попытка вызвать python -c "import sys; print('\n'.join(sys.stdlib_module_names))"
    let mut cmd = Command::new(python_path);
    cmd.args(["-c", "import sys; print('\\n'.join(getattr(sys, 'stdlib_module_names', ())))"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output();

    if let Ok(out) = output {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    stdlib.insert(trimmed.to_string());
                }
            }
        }
    }

    // Если Python вернул пусто (или старая версия), добавляем надежный базовый список стандартных библиотек
    if stdlib.is_empty() {
        let fallback = [
            "os", "sys", "re", "json", "math", "time", "datetime", "pathlib", "collections",
            "itertools", "functools", "urllib", "http", "io", "subprocess", "threading",
            "multiprocessing", "logging", "typing", "hashlib", "base64", "shutil", "glob",
            "tempfile", "sqlite3", "csv", "xml", "unittest", "unittest.mock", "argparse",
            "copy", "random", "string", "platform", "socket", "ssl", "uuid", "zipfile",
            "tarfile", "gzip", "bz2", "queue", "signal", "ctypes", "inspect", "traceback",
            "contextlib", "enum", "dataclasses", "asyncio", "heapq", "bisect", "shelve",
            "pickle", "marshal", "weakref", "gc", "builtins", "__future__", "site", "venv",
        ];
        for m in fallback {
            stdlib.insert(m.to_string());
        }
    }

    stdlib
}

/// Рекурсивный поиск зависимостей по файлу и его локальным импортам
pub fn analyze_python_script(script_path: &Path, python_path: &Path) -> Result<AnalysisResult, String> {
    let script_dir = script_path.parent().unwrap_or(Path::new("."));
    let stdlib = get_stdlib_modules(python_path);
    let pkg_map = get_package_map();

    let mut visited_files = HashSet::new();
    let mut files_to_scan = vec![script_path.to_path_buf()];

    let mut all_raw_imports = HashSet::new();
    let mut is_gui = false;

    // Регулярные выражения для парсинга операторов импорта:
    // 1) import foo, bar as b, baz
    // 2) from foo.bar import baz, qux
    let re_import = Regex::new(r"(?m)^\s*import\s+([^#\n]+)").map_err(|e| e.to_string())?;
    let re_from = Regex::new(r"(?m)^\s*from\s+([a-zA-Z0-9_\.]+)\s+import").map_err(|e| e.to_string())?;

    while let Some(current_file) = files_to_scan.pop() {
        let canonical = current_file.canonicalize().unwrap_or_else(|_| current_file.clone());
        if !visited_files.insert(canonical) {
            continue;
        }

        let content = match fs::read_to_string(&current_file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Не удалось прочитать {}: {}", current_file.display(), e);
                continue;
            }
        };

        // Поиск `import ...`
        for cap in re_import.captures_iter(&content) {
            if let Some(matched) = cap.get(1) {
                for item in matched.as_str().split(',') {
                    let token = item.trim().split_whitespace().next().unwrap_or("");
                    if !token.is_empty() {
                        let top_mod = token.split('.').next().unwrap_or(token);
                        all_raw_imports.insert(top_mod.to_string());
                    }
                }
            }
        }

        // Поиск `from ... import ...`
        for cap in re_from.captures_iter(&content) {
            if let Some(matched) = cap.get(1) {
                let token = matched.as_str().trim();
                let top_mod = token.split('.').next().unwrap_or(token);
                if !top_mod.is_empty() && !top_mod.starts_with('.') {
                    all_raw_imports.insert(top_mod.to_string());
                }
            }
        }

        // Проверяем, есть ли среди импортов локальные файлы в директории скрипта
        for imp in &all_raw_imports {
            let candidate_py = script_dir.join(format!("{}.py", imp));
            let candidate_dir = script_dir.join(imp).join("__init__.py");

            if candidate_py.exists() && !visited_files.contains(&candidate_py.canonicalize().unwrap_or_else(|_| candidate_py.clone())) {
                files_to_scan.push(candidate_py);
            } else if candidate_dir.exists() && !visited_files.contains(&candidate_dir.canonicalize().unwrap_or_else(|_| candidate_dir.clone())) {
                files_to_scan.push(candidate_dir);
            }
        }
    }

    // Фильтрация импортов: убираем stdlib и локальные модули
    let mut external_packages = HashSet::new();
    let mut detected_modules: Vec<String> = all_raw_imports.iter().cloned().collect();
    detected_modules.sort();

    for imp in &all_raw_imports {
        // Проверка на GUI
        if GUI_MODULES.contains(&imp.as_str()) {
            is_gui = true;
        }

        // Если это стандартная библиотека Python - пропускаем
        if stdlib.contains(imp) {
            continue;
        }

        // Если это локальный файл/папка скрипта - пропускаем
        let local_file = script_dir.join(format!("{}.py", imp));
        let local_dir = script_dir.join(imp);
        if local_file.exists() || (local_dir.is_dir() && local_dir.join("__init__.py").exists()) {
            continue;
        }

        // Сопоставление с PyPI пакетом
        if let Some(&pypi_name) = pkg_map.get(imp.as_str()) {
            if !pypi_name.is_empty() {
                external_packages.insert(pypi_name.to_string());
            }
        } else {
            // Если нет в словаре сопоставления, берем исходное имя модуля
            external_packages.insert(imp.clone());
        }
    }

    let mut pkg_list: Vec<String> = external_packages.into_iter().collect();
    pkg_list.sort();

    Ok(AnalysisResult {
        external_packages: pkg_list,
        detected_modules,
        is_gui,
        local_files: visited_files.into_iter().collect(),
    })
}
