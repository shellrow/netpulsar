use anyhow::Result;
use futures::{stream, StreamExt};
use rand::{seq::SliceRandom, thread_rng, Rng};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex};

use crate::model::scan::{HostScanProgress, HostScanReport, HostScanSetting, HostState};
use crate::probe::packet::{build_icmp_echo_bytes, parse_icmp_echo_v4, parse_icmp_echo_v6};
use crate::socket::icmp::{AsyncIcmpSocket, IcmpConfig, IcmpKind};
use crate::socket::SocketFamily;

pub const HOSTS_CONCURRENCY: usize = 256;

struct Pending {
    #[allow(dead_code)]
    ip: IpAddr,
    sent_at: Instant,
    tx: oneshot::Sender<u64>,
}

fn spawn_receiver(
    socket: Arc<AsyncIcmpSocket>,
    pending: Arc<Mutex<HashMap<IpAddr, Pending>>>,
    is_v6: bool,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 2048];
        loop {
            let Ok((n, addr)) = socket.recv_from(&mut buf).await else {
                // Error on recv, socket might be closed
                break;
            };
            let is_echo_reply = if !is_v6 {
                // IPv4
                parse_icmp_echo_v4(&buf[..n]).is_some()
            } else {
                // IPv6
                parse_icmp_echo_v6(&buf[..n]).is_some()
            };

            if is_echo_reply {
                let mut map = pending.lock().await;
                if let Some(p) = map.remove(&addr.ip()) {
                    let _ = p.tx.send(p.sent_at.elapsed().as_millis() as u64);
                }
            }
        }
    })
}

pub async fn host_scan(
    app: &AppHandle,
    run_id: &str,
    src_ipv4: Option<IpAddr>,
    src_ipv6: Option<IpAddr>,
    mut setting: HostScanSetting,
) -> Result<HostScanReport> {
    let total = setting.targets.len() as u32;
    let done_ctr = Arc::new(AtomicU32::new(0));
    let timeout = Duration::from_millis(setting.timeout_ms);
    let payload = setting
        .payload
        .clone()
        .unwrap_or_else(|| "np:hs".to_string());
    let concurrency = setting.concurrency.unwrap_or(HOSTS_CONCURRENCY);
    if !setting.ordered {
        setting.targets.shuffle(&mut thread_rng());
    }

    let socket_v4 = if setting.targets.iter().any(|ip| ip.is_ipv4()) {
        let mut cfg = IcmpConfig::new(IcmpKind::V4);
        cfg = cfg.with_ttl(setting.hop_limit.max(1) as u32);
        Some(Arc::new(AsyncIcmpSocket::new(&cfg).await?))
    } else {
        None
    };

    let socket_v6 = if setting.targets.iter().any(|ip| ip.is_ipv6()) {
        let mut cfg = IcmpConfig::new(IcmpKind::V6);
        cfg = cfg.with_hoplimit(setting.hop_limit.max(1) as u32);
        Some(Arc::new(AsyncIcmpSocket::new(&cfg).await?))
    } else {
        None
    };

    // pending map for each family
    let pending_v4: Arc<Mutex<HashMap<IpAddr, Pending>>> = Arc::new(Mutex::new(HashMap::new()));
    let pending_v6: Arc<Mutex<HashMap<IpAddr, Pending>>> = Arc::new(Mutex::new(HashMap::new()));

    // Spawn receiver tasks
    let rx_v4 = socket_v4
        .as_ref()
        .map(|s| spawn_receiver(s.clone(), pending_v4.clone(), false));
    let rx_v6 = socket_v6
        .as_ref()
        .map(|s| spawn_receiver(s.clone(), pending_v6.clone(), true));

    // Clone for tasks
    let socket_v4_for_tasks = socket_v4.clone();
    let socket_v6_for_tasks = socket_v6.clone();
    let pending_v4_for_tasks = pending_v4.clone();
    let pending_v6_for_tasks = pending_v6.clone();

    let app_cl = app.clone();
    let timeout_cl = timeout;
    let payload_cl = payload.clone();
    let count_cl = setting.count.max(1);
    let total_cl = total;

    let mut stream_send = stream::iter(setting.targets.clone().into_iter())
        .map(move |dst_ip| {
            let app = app_cl.clone();
            let socket_v4 = socket_v4_for_tasks.clone();
            let socket_v6 = socket_v6_for_tasks.clone();
            let pending_v4 = pending_v4_for_tasks.clone();
            let pending_v6 = pending_v6_for_tasks.clone();
            let timeout = timeout_cl;
            let payload = payload_cl.clone();
            let cnt = count_cl;
            let total = total_cl;
            let done_ctr = done_ctr.clone();
            let src_ipv4 = src_ipv4;
            let src_ipv6 = src_ipv6;

            async move {
                // If no suitable socket, emit unreachable immediately
                let (sock_opt, pending_map, src_ip) = match SocketFamily::from_ip(&dst_ip) {
                    SocketFamily::IPV4 => (
                        socket_v4.clone(),
                        pending_v4.clone(),
                        src_ipv4.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
                    ),
                    SocketFamily::IPV6 => (
                        socket_v6.clone(),
                        pending_v6.clone(),
                        src_ipv6.unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
                    ),
                };

                let Some(sock) = sock_opt else {
                    let done = done_ctr.fetch_add(1, Ordering::Relaxed) + 1;
                    let p = HostScanProgress {
                        ip_addr: dst_ip,
                        state: HostState::Unreachable,
                        rtt_ms: None,
                        message: Some("no suitable socket for IP family".into()),
                        done,
                        total,
                    };
                    let _ = app.emit("hostscan:progress", p.clone());
                    return p;
                };

                let target = SocketAddr::new(dst_ip, 0);
                let mut best_rtt: Option<u64> = None;
                let mut last_err: Option<String> = None;

                for seq in 1..=cnt {
                    // Regist pending
                    let id: u16 = rand::thread_rng().gen();
                    let (tx, rx) = oneshot::channel::<u64>();

                    {
                        let mut map = pending_map.lock().await;
                        map.insert(
                            dst_ip,
                            Pending {
                                ip: dst_ip,
                                sent_at: Instant::now(),
                                tx,
                            },
                        );
                    }

                    // Build ICMP Echo Request packet
                    let pkt =
                        build_icmp_echo_bytes(src_ip, dst_ip, id, seq as u16, payload.as_bytes());

                    // Send ICMP Echo Request
                    if let Err(e) = sock.send_to(&pkt, target).await {
                        let mut map = pending_map.lock().await;
                        map.remove(&dst_ip);
                        last_err = Some(format!("send error: {}", e));
                        continue;
                    }

                    // Wait for reply or timeout
                    match tokio::time::timeout(timeout, rx).await {
                        Ok(Ok(rtt)) => {
                            best_rtt = Some(best_rtt.map_or(rtt, |b| b.min(rtt)));
                            break;
                        }
                        Ok(Err(_canceled)) => {
                            last_err = Some("wait canceled".into());
                        }
                        Err(_to) => {
                            let mut map = pending_map.lock().await;
                            map.remove(&dst_ip);
                            last_err = Some(format!("timeout (>{}ms)", timeout.as_millis()));
                        }
                    }
                }

                let done = done_ctr.fetch_add(1, Ordering::Relaxed) + 1;
                let p = if let Some(rtt) = best_rtt {
                    HostScanProgress {
                        ip_addr: dst_ip,
                        state: HostState::Alive,
                        rtt_ms: Some(rtt),
                        message: None,
                        done,
                        total,
                    }
                } else {
                    HostScanProgress {
                        ip_addr: dst_ip,
                        state: HostState::Unreachable,
                        rtt_ms: None,
                        message: last_err,
                        done,
                        total,
                    }
                };
                let _ = app.emit("hostscan:progress", p.clone());
                p
            }
        })
        .buffer_unordered(concurrency);

    // Collect results
    let mut alive: Vec<(IpAddr, u64)> = Vec::new();
    let mut unreachable: Vec<IpAddr> = Vec::new();

    while let Some(p) = stream_send.next().await {
        match p.state {
            HostState::Alive => alive.push((p.ip_addr, p.rtt_ms.unwrap_or(0))),
            HostState::Unreachable => unreachable.push(p.ip_addr),
        }
    }

    // Drop sockets to gracefully terminate receiver tasks
    drop(socket_v4);
    drop(socket_v6);
    if let Some(h) = rx_v4 {
        let _ = h.abort();
    }
    if let Some(h) = rx_v6 {
        let _ = h.abort();
    }

    // Report results
    let report = HostScanReport {
        run_id: run_id.to_string(),
        alive,
        unreachable,
        total,
    };
    let _ = app.emit("hostscan:done", report.clone());
    Ok(report)
}
