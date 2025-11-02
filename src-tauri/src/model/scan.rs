use netdev::MacAddr;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PortScanProtocol {
    Tcp,
    Quic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TargetPortsPreset {
    Common,
    WellKnown,
    Full,
    Top1000,
    Custom,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum PortState {
    Open,
    Closed,
    Filtered,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PortScanStartPayload {
    pub run_id: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PortScanSample {
    pub ip_addr: IpAddr,
    pub port: u16,
    pub state: PortState,
    pub rtt_ms: Option<u64>,
    pub message: Option<String>,
    pub service_name: Option<String>,
    pub done: u32,
    pub total: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PortScanReport {
    pub run_id: String,
    pub ip_addr: IpAddr,
    pub hostname: Option<String>,
    pub protocol: PortScanProtocol,
    pub samples: Vec<PortScanSample>,
}

/// Settings for a port scan operation
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PortScanSetting {
    pub ip_addr: IpAddr,
    pub hostname: Option<String>,
    pub target_ports_preset: TargetPortsPreset,
    pub user_ports: Vec<u16>,
    pub protocol: PortScanProtocol,
    pub timeout_ms: u64,
    pub ordered: bool,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum HostState {
    Alive,
    Unreachable,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostScanSetting {
    pub targets: Vec<IpAddr>,
    pub hop_limit: u8,
    pub timeout_ms: u64,
    pub count: u32,
    pub payload: Option<String>,
    pub ordered: bool,
    pub concurrency: Option<usize>,
}

impl HostScanSetting {
    pub fn neighbor_scan_default(iface: &netdev::Interface) -> Self {
        let mut targets: Vec<IpAddr> = Vec::new();
        if let Some(gw) = &iface.gateway {
            if let Some(ipv4) = gw.ipv4.first() {
                match netdev::ipnet::Ipv4Net::new(*ipv4, 24) {
                    Ok(ipv4net) => {
                        for ip in ipv4net.hosts() {
                            targets.push(IpAddr::V4(ip));
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        HostScanSetting {
            targets,
            hop_limit: 64,
            timeout_ms: 1000,
            count: 1,
            payload: Some("np:neigh".to_string()),
            ordered: true,
            concurrency: Some(100),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HostScanStartPayload {
    pub run_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostScanProgress {
    pub ip_addr: IpAddr,
    pub state: HostState,
    pub rtt_ms: Option<u64>,
    pub message: Option<String>,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostScanReport {
    pub run_id: String,
    pub alive: Vec<(IpAddr, u64)>, // (IP, RTT)
    pub unreachable: Vec<IpAddr>,
    pub total: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NeighborHost {
    pub ip_addr: IpAddr,
    pub mac_addr: Option<MacAddr>,
    pub vendor: Option<String>,
    pub rtt_ms: Option<u64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NeighborScanReport {
    pub run_id: String,
    pub neighbors: Vec<NeighborHost>,
    pub total: u32,
}
