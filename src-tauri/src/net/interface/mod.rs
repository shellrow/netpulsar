use netdev::Interface;

pub fn list_interfaces() -> Vec<Interface> {
    netdev::get_interfaces()
}

pub fn get_display_name(iface: &Interface) -> String {
    // On Windows, use the friendly name if available
    #[cfg(target_os = "windows")]
    {
        if let Some(friendly_name) = &iface.friendly_name {
            return friendly_name.clone();
        }
    }
    iface.name.clone()
}
