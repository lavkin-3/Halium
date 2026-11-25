use std::collections::HashMap;

/// Получить словарь соответствия имени импорта -> имени пакета на PyPI.
/// Содержит более 45 популярных библиотек с различающимися именами.
pub fn get_package_map() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    
    // Компьютерное зрение и обработка изображений
    map.insert("cv2", "opencv-python");
    map.insert("PIL", "Pillow");
    map.insert("skimage", "scikit-image");
    map.insert("fitz", "PyMuPDF");
    map.insert("imageio", "imageio");
    
    // Парсинг, веб и сети
    map.insert("bs4", "beautifulsoup4");
    map.insert("dotenv", "python-dotenv");
    map.insert("telebot", "pyTelegramBotAPI");
    map.insert("telegram", "python-telegram-bot");
    map.insert("discord", "discord.py");
    map.insert("jwt", "PyJWT");
    map.insert("websocket", "websocket-client");
    map.insert("googleapiclient", "google-api-python-client");
    map.insert("dns", "dnspython");
    
    // Форматы данных и документы
    map.insert("yaml", "PyYAML");
    map.insert("docx", "python-docx");
    map.insert("pptx", "python-pptx");
    map.insert("dateutil", "python-dateutil");
    map.insert("openpyxl", "openpyxl");
    map.insert("xlsxwriter", "XlsxWriter");
    map.insert("xlrd", "xlrd");
    map.insert("serial", "pyserial");
    
    // Криптография и безопасность
    map.insert("Crypto", "pycryptodome");
    map.insert("nacl", "PyNaCl");
    map.insert("OpenSSL", "pyOpenSSL");
    map.insert("jose", "python-jose");
    
    // Data Science и ML
    map.insert("sklearn", "scikit-learn");
    map.insert("torch", "torch");
    map.insert("torchvision", "torchvision");
    map.insert("torchaudio", "torchaudio");
    map.insert("seaborn", "seaborn");
    map.insert("matplotlib", "matplotlib");
    map.insert("pandas", "pandas");
    map.insert("numpy", "numpy");
    map.insert("scipy", "scipy");
    
    // Windows API и системные утилиты
    map.insert("win32api", "pywin32");
    map.insert("win32con", "pywin32");
    map.insert("win32gui", "pywin32");
    map.insert("win32process", "pywin32");
    map.insert("win32com", "pywin32");
    map.insert("pythoncom", "pywin32");
    map.insert("psutil", "psutil");
    map.insert("win10toast", "win10toast");
    map.insert("winshell", "winshell");
    
    // Базы данных и ORM
    map.insert("psycopg2", "psycopg2-binary");
    map.insert("pymongo", "pymongo");
    map.insert("redis", "redis");
    map.insert("sqlalchemy", "SQLAlchemy");
    map.insert("tortoise", "tortoise-orm");
    
    // GUI библиотеки
    map.insert("tkinter", ""); // Встроено в стандартный Python для Windows
    map.insert("customtkinter", "customtkinter");
    map.insert("PyQt5", "PyQt5");
    map.insert("PyQt6", "PyQt6");
    map.insert("PySide2", "PySide2");
    map.insert("PySide6", "PySide6");
    map.insert("pygame", "pygame");
    map.insert("kivy", "kivy");
    map.insert("wx", "wxPython");
    map.insert("flet", "flet");

    map
}

/// Список GUI библиотек для автоматического добавления флага `--noconsole`
pub const GUI_MODULES: &[&str] = &[
    "tkinter",
    "customtkinter",
    "PyQt5",
    "PyQt6",
    "PySide2",
    "PySide6",
    "pygame",
    "kivy",
    "wx",
    "flet",
];

/// Пакеты, требующие `--collect-all` в PyInstaller
pub const COLLECT_ALL_PACKAGES: &[&str] = &[
    "customtkinter",
    "certifi",
    "imageio",
    "scipy",
    "sklearn",
    "matplotlib",
    "torch",
];
