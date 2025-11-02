use crate::net::sys::os::system_info;
use crate::model::SysInfo;

#[tauri::command]
pub fn get_sys_info() -> SysInfo {
    system_info()
}
