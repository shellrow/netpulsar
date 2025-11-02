use anyhow::Result;
use futures::{stream, StreamExt};
use rand::{seq::SliceRandom, thread_rng};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
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

    let mut tasks = stream::iter(ports.into_iter())
        .map(|port| {
            let app = app.clone();
            let done_ctr = done_ctr.clone();
            let hostname_opt = setting.hostname.clone();

            async move {
                let family = if ip.is_ipv4() {
                    crate::socket::SocketFamily::IPV4
                } else {
                    crate::socket::SocketFamily::IPV6
                };

                let quic_cfg = crate::socket::quic::QuicConfig {
                    skip_verify: true,
                    alpn: vec![b"h3".to_vec(), b"hq-29".to_vec(), b"hq-interop".to_vec()],
                    family,
                };

                let (state, rtt_ms, msg) =
                    match crate::socket::quic::AsyncQuicSocket::from_config(&quic_cfg) {
                        Ok(ep) => {
                            let server_name =
                                hostname_opt.clone().unwrap_or_else(|| ip.to_string());
                            let start = Instant::now();
                            match ep
                                .connect_timeout(&SocketAddr::new(ip, port), &server_name, timeout)
                                .await
                            {
                                Ok(conn) => {
                                    conn.close(0u32.into(), b"scan");
                                    (
                                        PortState::Open,
                                        Some(start.elapsed().as_millis() as u64),
                                        None,
                                    )
                                }
                                Err(e) => {
                                    let st = if let Some(ioe) = e.downcast_ref::<std::io::Error>() {
                                        if ioe.kind() == std::io::ErrorKind::TimedOut {
                                            PortState::Filtered
                                        } else {
                                            PortState::Closed
                                        }
                                    } else {
                                        PortState::Closed
                                    };
                                    (st, None, Some(e.to_string()))
                                }
                            }
                        }
                        Err(e) => (
                            PortState::Filtered,
                            None,
                            Some(format!("quic endpoint error: {}", e)),
                        ),
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
    let mut open_samples = Vec::new();
    let udp_service_db = ndb_udp_service::UdpServiceDb::bundled();
    while let Some(sample) = tasks.next().await {
        if matches!(sample.state, PortState::Open) {
            let mut sample = sample;
            match udp_service_db.get(sample.port) {
                Some(entry) => {
                    sample.service_name = Some(entry.name.clone());
                }
                None => sample.service_name = None,
            }
            open_samples.push(sample);
        }
    }

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
