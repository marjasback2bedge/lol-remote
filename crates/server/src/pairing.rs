//! Terminal pairing output (spec §7.6).

use qrcode::render::unicode::Dense1x2;
use qrcode::QrCode;

use crate::config::Config;

fn qr(data: &str) -> String {
    match QrCode::new(data) {
        Ok(code) => code
            .render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
        Err(e) => format!("(could not render QR code: {e})"),
    }
}

pub fn print(config: &Config, mdns_url: &str, ip_url: &str) {
    let token = &config.auth_token;
    let ip_link = format!("{ip_url}/?t={token}");
    println!();
    println!("━━━━━━━━━━━━━━━━ lol-remote 配對 ━━━━━━━━━━━━━━━━");
    println!("用手機掃描以開啟遙控網頁（手機需連同一個區網）：");
    println!("{}", qr(&ip_link));
    println!("   {ip_link}");
    println!("   {mdns_url}/?t={token}");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
}
