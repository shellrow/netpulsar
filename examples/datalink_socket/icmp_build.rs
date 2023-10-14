use cross_socket::datalink::interface::Interface;
use cross_socket::packet::builder::PacketBuilder;
use cross_socket::packet::ethernet::{EtherType, EthernetPacketBuilder};
use cross_socket::packet::icmp::IcmpPacketBuilder;
use cross_socket::packet::ip::IpNextLevelProtocol;
use cross_socket::packet::ipv4::Ipv4PacketBuilder;
use cross_socket::socket::DataLinkSocket;
use default_net::interface::MacAddr;
use std::net::Ipv4Addr;
use std::env;

// Send ICMP Echo Request packets to 1.1.1.1 and check reply
fn main() {
    let interface: Interface = match env::args().nth(1) {
        Some(n) => {
            // Use interface specified by user
            let interfaces: Vec<Interface> = default_net::get_interfaces();
            let interface: Interface = interfaces
                .into_iter()
                .find(|interface| interface.name == n)
                .expect("Failed to get interface information");
            interface
        },
        None => {
            // Use default interface
            default_net::get_default_interface().expect("Failed to get default interface information")
        }
    };
    let use_tun: bool = interface.is_tun();
    let src_ip: Ipv4Addr = interface.ipv4[0].addr;
    let dst_ip: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);
    // Create new socket
    let mut socket: DataLinkSocket = DataLinkSocket::new(interface, false).unwrap();
    // Packet builder for ICMP Echo Request
    let mut packet_builder = PacketBuilder::new();
    let ethernet_packet_builder = EthernetPacketBuilder {
        src_mac: if use_tun { MacAddr::zero() } else { socket.interface.mac_addr.clone().unwrap() },
        dst_mac: if use_tun { MacAddr::zero() } else { socket.interface.gateway.clone().unwrap().mac_addr },
        ether_type: EtherType::Ipv4,
    };
    packet_builder.set_ethernet(ethernet_packet_builder);
    let ipv4_packet_builder = Ipv4PacketBuilder::new(src_ip, dst_ip, IpNextLevelProtocol::Icmp);
    packet_builder.set_ipv4(ipv4_packet_builder);
    let mut icmp_packet_builder = IcmpPacketBuilder::new(src_ip, dst_ip);
    icmp_packet_builder.icmp_type = cross_socket::packet::icmp::IcmpType::EchoRequest;
    packet_builder.set_icmp(icmp_packet_builder);

    // Send ICMP Echo Request packets to 1.1.1.1
    let packet = if use_tun {packet_builder.ip_packet()} else {packet_builder.packet()};
    match socket.send_to(&packet) {
        Ok(packet_len) => {
            println!("Sent {} bytes", packet_len);
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    // Receive packets
    println!("Waiting for ICMP Echo Reply... ");
    loop {
        match socket.receive() {
            Ok(packet) => {
                if use_tun {
                    let ip_packet =
                        cross_socket::packet::ipv4::Ipv4Packet::from_bytes(&packet);
                    if ip_packet.next_protocol != IpNextLevelProtocol::Icmp
                        || ip_packet.source != dst_ip
                    {
                        continue;
                    }
                    println!("Received {} bytes from {}", packet.len(), ip_packet.source);
                    let icmp_packet =
                        cross_socket::packet::icmp::IcmpPacket::from_bytes(&ip_packet.payload);
                    println!("Packet: {:?}", icmp_packet);
                    break;
                }else {
                    let ethernet_packet =
                    cross_socket::packet::ethernet::EthernetPacket::from_bytes(&packet);
                    if ethernet_packet.ethertype != EtherType::Ipv4 {
                        continue;
                    }
                    let ip_packet =
                        cross_socket::packet::ipv4::Ipv4Packet::from_bytes(&ethernet_packet.payload);
                    if ip_packet.next_protocol != IpNextLevelProtocol::Icmp
                        || ip_packet.source != dst_ip
                    {
                        continue;
                    }
                    println!("Received {} bytes from {}", packet.len(), ip_packet.source);
                    let icmp_packet =
                        cross_socket::packet::icmp::IcmpPacket::from_bytes(&ip_packet.payload);
                    println!("Packet: {:?}", icmp_packet);
                    break;
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}
