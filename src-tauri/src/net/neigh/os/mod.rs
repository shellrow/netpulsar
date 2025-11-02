#[cfg(target_vendor = "apple")]
mod darwin;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_vendor = "apple")]
pub use self::darwin::get_arp_table;

#[cfg(target_os = "windows")]
pub use self::windows::get_arp_table;

#[cfg(target_os = "linux")]
pub use self::linux::get_arp_table;
