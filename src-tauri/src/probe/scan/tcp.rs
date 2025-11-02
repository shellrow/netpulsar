use anyhow::Result;
use futures::{stream, StreamExt};
use rand::{seq::SliceRandom, thread_rng};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::model::scan::{PortScanReport, PortScanSample, PortScanSetting, PortState};
use crate::probe::scan::{expand_ports, PORTS_CONCURRENCY};

pub async fn port_scan(
    app: &AppHandle,
    run_id: &str,
    _src_ip: IpAddr,
    setting: PortScanSetting,
) -> Result<PortScanReport> {
    let mut ports = expand_ports(&setting.target_ports_preset, &setting.user_ports);
    if !setting.ordered {
        ports.shuffle(&mut thread_rng());
    }

    let app = app.clone();
    let ip = setting.ip_addr;
    let timeout = Duration::from_millis(setting.timeout_ms);

    let total = ports.len() as u32;
    let done_ctr = Arc::new(AtomicU32::new(0));

    // Create tasks for each port
    // Collect results as they complete
    let mut tasks = stream::iter(ports.into_iter())
        .map(|port| {
            let app = app.clone();
            let done_ctr = done_ctr.clone();
            async move {
                let cfg = if ip.is_ipv4() {
                    crate::socket::tcp::TcpConfig::v4_stream()
                } else {
                    crate::socket::tcp::TcpConfig::v6_stream()
                };
                let sock_addr = SocketAddr::new(ip, port);
                let sock = match crate::socket::tcp::AsyncTcpSocket::from_config(&cfg) {
                    Ok(s) => s,
                    Err(e) => {
                        let done = done_ctr.fetch_add(1, Ordering::Relaxed) + 1;
                        let sample = PortScanSample {
                            ip_addr: ip,
                            port,
                            state: PortState::Filtered,
                            rtt_ms: None,
                            message: Some(format!("tcp socket error: {}", e)),
                            service_name: None,
                            done,
                            total,
                        };
                        let _ = app.emit("portscan:progress", sample.clone());
                        return sample;
                    }
                };

                let start = Instant::now();

                let (state, rtt_ms, msg) = match sock.connect_timeout(sock_addr, timeout).await {
                    Ok(stream) => {
                        drop(stream);
                        (
                            PortState::Open,
                            Some(start.elapsed().as_millis() as u64),
                            None,
                        )
                    }
                    Err(e) => {
                        use std::io::ErrorKind as E;
                        let st = match e.kind() {
                            E::TimedOut => PortState::Filtered,
                            E::ConnectionRefused | E::ConnectionReset | E::NotConnected => {
                                PortState::Closed
                            }
                            E::NetworkUnreachable | E::HostUnreachable | E::AddrNotAvailable => {
                                PortState::Filtered
                            }
                            _ => PortState::Closed,
                        };
                        (st, None, Some(e.to_string()))
                    }
                };

                let done = done_ctr.fetch_add(1, Ordering::Relaxed) + 1;
                let sample = PortScanSample {
                    ip_addr: ip,
                    port,
                    state,
                    rtt_ms,
                    message: msg,
                    service_name: None,
                    done,
                    total,
                };

                let _ = app.emit("portscan:progress", sample.clone());
                sample
            }
        })
        .buffer_unordered(PORTS_CONCURRENCY);

    // Collect only Open samples
    let mut open_samples: Vec<PortScanSample> = Vec::new();
    let tcp_service_db = ndb_tcp_service::TcpServiceDb::bundled();
    while let Some(sample) = tasks.next().await {
        if matches!(sample.state, PortState::Open) {
            let mut sample = sample;
            match tcp_service_db.get(sample.port) {
                Some(entry) => {
                    sample.service_name = Some(entry.name.clone());
                },
                None => sample.service_name = None,
            }
            open_samples.push(sample);
        }
    }

    // Sort samples by port
    open_samples.sort_by_key(|s| s.port);

    let report = PortScanReport {
        run_id: run_id.to_string(),
        ip_addr: setting.ip_addr,
        hostname: setting.hostname.clone(),
        protocol: setting.protocol,
        samples: open_samples,
    };

    let _ = app.emit("portscan:done", report.clone());
    Ok(report)
}
