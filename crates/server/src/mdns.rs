//! mDNS advertisement (spec §7.6). macOS already answers for `<host>.local`;
//! this mostly matters on Windows.

use std::net::IpAddr;

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// Keep the returned daemon alive for as long as the advertisement should run.
pub fn advertise(host: &str, ip: IpAddr, port: u16) -> anyhow::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    let info = ServiceInfo::new(
        "_http._tcp.local.",
        "lol-remote",
        &format!("{host}.local."),
        ip,
        port,
        &[("path", "/")][..],
    )?;
    daemon.register(info)?;
    Ok(daemon)
}
