mod app;
mod command;
mod config;
mod fs;
mod net;
mod log;
mod model;
mod probe;
mod service;
mod socket;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}
