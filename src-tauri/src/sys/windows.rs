use std::collections::HashMap;
use winreg::enums::RegDisposition;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;
use tauri::Manager;
use crate::deps::{NPCAP_DIST_BASE_URL, NPCAP_INSTALLER_FILENAME};

/// Application information on system
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub display_name: String,
    pub display_version: String,
    pub uninstall_string: String,
}

pub fn get_os_bit() -> String {
    if cfg!(target_pointer_width = "32") {
        return "32-bit".to_owned();
    } else if cfg!(target_pointer_width = "64") {
        return "64-bit".to_owned();
    } else {
        return "unknown".to_owned();
    }
}

pub fn get_install_path(install_dir_name: &str) -> String {
    match home::home_dir() {
        Some(path) => {
            let path: String = format!("{}\\{}", path.display(), install_dir_name);
            path
        }
        None => String::new(),
    }
}

// Get software installation status
pub fn software_installed(software_name: String) -> bool {
    let hklm: RegKey = RegKey::predef(HKEY_LOCAL_MACHINE);
    let os_bit: String = get_os_bit();
    let npcap_key: RegKey = if os_bit == "32-bit" {
        match hklm.open_subkey(format!("SOFTWARE\\{}", software_name)) {
            Ok(key) => key,
            Err(_) => return false,
        }
    } else {
        match hklm.open_subkey(format!("SOFTWARE\\WOW6432Node\\{}", software_name)) {
            Ok(key) => key,
            Err(_) => return false,
        }
    };
    let _version: String = npcap_key.get_value("").unwrap_or(String::new());
    true
}

#[allow(dead_code)]
pub fn get_installed_apps() -> HashMap<String, AppInfo> {
    let hklm: RegKey = winreg::RegKey::predef(HKEY_LOCAL_MACHINE);
    let uninstall_key: RegKey = hklm
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall")
        .expect("key is missing");

    let mut apps: HashMap<String, AppInfo> = HashMap::new();
    for key in uninstall_key.enum_keys() {
        let key = match key {
            Ok(key) => key,
            Err(_) => continue,
        };
        //let key = key.unwrap();
        let subkey: RegKey = uninstall_key
            .open_subkey(key.clone())
            .expect("key is missing");
        let app: AppInfo = AppInfo {
            display_name: subkey.get_value("DisplayName").unwrap_or(String::new()),
            display_version: subkey.get_value("DisplayVersion").unwrap_or(String::new()),
            uninstall_string: subkey.get_value("UninstallString").unwrap_or(String::new()),
        };
        apps.insert(key, app);
    }
    apps
}

#[allow(dead_code)]
pub fn app_installed(app_name: String) -> bool {
    let hklm: RegKey = winreg::RegKey::predef(HKEY_LOCAL_MACHINE);
    let uninstall_key: RegKey = hklm
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall")
        .expect("key is missing");

    for key in uninstall_key.enum_keys() {
        let key = match key {
            Ok(key) => key,
            Err(_) => continue,
        };
        //let key = key.unwrap();
        let subkey: RegKey = uninstall_key
            .open_subkey(key.clone())
            .expect("key is missing");
        let display_name: String = subkey.get_value("DisplayName").unwrap_or(String::new());
        if display_name == app_name {
            return true;
        }
    }
    false
}

#[allow(dead_code)]
pub fn check_env_path(dir_path: &str) -> bool {
    let reg_key: winreg::RegKey = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", winreg::enums::KEY_READ)
        .unwrap();
    let reg_value: String = reg_key.get_value("Path").unwrap_or(String::new());
    reg_value.contains(dir_path)
}

#[allow(dead_code)]
pub fn add_env_path(dir_path: &str) -> std::io::Result<()> {
    let hkcu: winreg::RegKey = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (path, _): (winreg::RegKey, RegDisposition) = hkcu.create_subkey("Environment")?;
    let mut path_value: String = path
        .get_value::<String, &str>("Path")
        .unwrap_or(String::new());
    if !path_value.is_empty() {
        path_value.push(';');
    }
    path_value.push_str(&std::path::Path::new(dir_path).to_str().unwrap());
    println!("{}", path_value);
    path.set_value("Path", &path_value)
}

pub fn check_env_lib_path(dir_path: &str) -> bool {
    let reg_key: winreg::RegKey = match winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", winreg::enums::KEY_READ)
    {
        Ok(key) => key,
        Err(_) => return false,
    };
    let reg_value: String = reg_key.get_value("LIB").unwrap_or(String::new());
    reg_value.contains(dir_path)
}

pub fn add_env_lib_path(dir_path: &str) -> std::io::Result<()> {
    let hkcu: winreg::RegKey = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (path, _): (winreg::RegKey, RegDisposition) = hkcu.create_subkey("Environment")?;
    let mut path_value: String = path
        .get_value::<String, &str>("LIB")
        .unwrap_or(String::new());
    if !path_value.is_empty() {
        path_value.push(';');
    }
    path_value.push_str(&std::path::Path::new(dir_path).to_str().unwrap());
    println!("{}", path_value);
    path.set_value("LIB", &path_value)
}

pub fn get_env_lib() -> String {
    let reg_key: winreg::RegKey = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", winreg::enums::KEY_READ)
        .unwrap();
    let reg_value: String = reg_key.get_value("LIB").unwrap_or(String::new());
    if !reg_value.is_empty() {
        return reg_value;
    }
    let reg_key: winreg::RegKey = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
            winreg::enums::KEY_READ,
        )
        .unwrap();
    let reg_value: String = reg_key.get_value("LIB").unwrap_or(String::new());
    reg_value
}

pub async fn download_npcap(app_handle: &tauri::AppHandle) -> Result<u64, Box<dyn std::error::Error>> {
    let mut download_content_length: u64 = 0;
    let mut downloaded_bytes: u64 = 0;
    let npcap_installer_url = format!("{}{}", NPCAP_DIST_BASE_URL, NPCAP_INSTALLER_FILENAME);
    let download_dir = crate::sys::get_download_dir_path().ok_or("Failed to get download dir")?;
    // Check and create download dir
    if !download_dir.exists() {
        std::fs::create_dir_all(&download_dir)?;
    }
    let npcap_target_path: std::path::PathBuf = download_dir.join(NPCAP_INSTALLER_FILENAME);
    // Download npcap installer if not exists
    if std::path::Path::new(&npcap_target_path).exists() {
        // return file size if file exists
        return Ok(std::fs::metadata(&npcap_target_path)?.len());
    }
    let installer_save_path: std::path::PathBuf = npcap_target_path.clone();
    // create a channel for progress
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(100);
    // spawn a task to handle the progress
    tokio::spawn(async move {
        let _ = crate::net::http::download_file_with_progress(npcap_installer_url, installer_save_path, progress_tx).await;
    });
    // Emit progress
    while let Some(progress) = progress_rx.recv().await {
        match progress {
            crate::net::http::DownloadProgress::ContentLength(content_length) => {
                download_content_length = content_length; 
            }
            crate::net::http::DownloadProgress::Downloaded(downloaded) => {
                downloaded_bytes = downloaded;
            }
        }
        match app_handle.emit_all("download_progress", progress) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Error: {:?}", e);
            }
        }
    }
    if download_content_length == downloaded_bytes {
        Ok(download_content_length)
    }else{
        Err("Download failed".into())
    }
}

pub fn run_npcap_installer() -> Result<(), Box<dyn std::error::Error>> {
    let download_dir = crate::sys::get_download_dir_path().ok_or("Failed to get download dir")?;
    let npcap_target_path: std::path::PathBuf = download_dir.join(NPCAP_INSTALLER_FILENAME);
    if !std::path::Path::new(&npcap_target_path).exists() {
        return Err("Npcap installer not found".into());
    }
    // Verify the checksum of the downloaded npcap installer
    match crate::deps::verify_installer_checksum(&npcap_target_path) {
        Ok(_) => {},
        Err(e) => {
            return Err(e);
        },
    }
    // Run npcap installer
    match crate::deps::run_npcap_installer(&npcap_target_path) {
        Ok(_) => {
            Ok(())
        }
        Err(e) => {
            Err(e)
        }
    }
}