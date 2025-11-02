use std::{collections::HashMap, io, net::IpAddr};

use netdev::MacAddr;

mod os;

pub fn get_arp_table() -> io::Result<HashMap<IpAddr, MacAddr>> {
    os::get_arp_table()
}
