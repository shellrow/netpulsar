use std::net::IpAddr;

use netdev::Interface;
use tauri::{AppHandle, Emitter};

use crate::model::scan::{
    HostScanReport, HostScanSetting, NeighborScanReport, PortScanProtocol, PortScanReport, PortScanSetting
};

#[tauri::command]
pub async fn port_scan(app: AppHandle, setting: PortScanSetting) -> Result<PortScanReport, String> {
    let default_interface: Interface = netdev::get_default_interface()
        .map_err(|e| format!("Failed to get default interface: {}", e))?;
    let src_ip = match setting.ip_addr {
        std::net::IpAddr::V4(_) => {
            // Pick first IPv4 address of default interface
            let ipv4 = default_interface
                .ipv4_addrs()
                .into_iter()
                .next()
                .ok_or("No IPv4 address found on default interface")?;
            IpAddr::V4(ipv4)
        }
        std::net::IpAddr::V6(_) => {
            // Pick first IPv6 address of default interface
            let ipv6 = default_interface
                .ipv6_addrs()
                .into_iter()
                .next()
                .ok_or("No IPv6 address found on default interface")?;
            IpAddr::V6(ipv6)
        }
    };
    let run_id = uuid::Uuid::new_v4().to_string();
    // Start event
    let _ = app.emit(
        "portscan:start",
        crate::model::scan::PortScanStartPayload {
            run_id: run_id.clone(),
        },
    );

    match setting.protocol {
        PortScanProtocol::Tcp => crate::probe::scan::tcp::port_scan(&app, &run_id, src_ip, setting)
            .await
            .map_err(|e| e.to_string()),
        PortScanProtocol::Quic => {
            crate::probe::scan::quic::port_scan(&app, &run_id, src_ip, setting)
                .await
                .map_err(|e| e.to_string())
        }
    }
}

#[tauri::command]
pub async fn host_scan(app: AppHandle, setting: HostScanSetting) -> Result<HostScanReport, String> {
    let run_id = uuid::Uuid::new_v4().to_string();

    let default_if = netdev::get_default_interface().map_err(|e| e.to_string())?;

    let src_ipv4_opt = default_if
        .ipv4_addrs()
        .into_iter()
        .next()
        .map(std::net::IpAddr::V4);
    let src_ipv6_opt = default_if
        .ipv6_addrs()
        .into_iter()
        .next()
        .map(std::net::IpAddr::V6);

    let _ = app.emit(
        "hostscan:start",
        crate::model::scan::HostScanStartPayload {
            run_id: run_id.clone(),
        },
    );
    crate::probe::scan::icmp::host_scan(&app, &run_id, src_ipv4_opt, src_ipv6_opt, setting)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn neighbor_scan(app: AppHandle, iface_name: Option<String>) -> Result<NeighborScanReport, String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let _ = app.emit(
        "neighborscan:start",
        run_id.clone(),
    );
    let iface = if let Some(name) = iface_name {
        netdev::get_interfaces()
            .into_iter()
            .find(|i| i.name == name)
            .ok_or_else(|| format!("interface not found: {name}"))?
    } else {
        netdev::get_default_interface().map_err(|e| e.to_string())?
    };
    crate::probe::scan::neigh::neighbor_scan(&app, &run_id, iface)
        .await
        .map_err(|e| e.to_string())
}
