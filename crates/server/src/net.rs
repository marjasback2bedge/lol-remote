//! Host name and LAN address discovery.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::process::Command;

/// The machine's Bonjour/NetBIOS name, without `.local`.
pub fn system_hostname() -> String {
    let from_cmd = |cmd: &str, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let name = if cfg!(target_os = "macos") {
        from_cmd("scutil", &["--get", "LocalHostName"])
    } else if cfg!(target_os = "windows") {
        std::env::var("COMPUTERNAME").ok()
    } else {
        None
    }
    .or_else(|| from_cmd("hostname", &[]))
    .unwrap_or_else(|| "localhost".into());
    name.trim_end_matches(".local").to_owned()
}

/// The address other devices on the LAN can reach us at. Connecting a UDP
/// socket sends no packets; it only makes the OS pick the outbound interface.
pub fn lan_ip() -> IpAddr {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|s| {
            s.connect((Ipv4Addr::new(192, 168, 0, 1), 80))?;
            s.local_addr()
        })
        .map(|a| a.ip())
        .ok()
        .filter(|ip| !ip.is_unspecified())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}
