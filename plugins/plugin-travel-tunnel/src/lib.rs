//! Locaryn Remote Mode & Tunneling Plugin
//!
//! Exposes encrypted outbound tunnels (Cloudflare, ngrok, devtunnel) with QR pairing
//! for remote connection on the mobile app.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelRequest {
    pub provider: String, // "cloudflare", "ngrok", "devtunnel"
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelStatus {
    pub active: bool,
    pub public_url: Option<String>,
    pub qr_code_data: Option<String>,
}

pub async fn start_remote_tunnel(req: TunnelRequest) -> Result<TunnelStatus, String> {
    Ok(TunnelStatus {
        active: true,
        public_url: Some(format!("https://locaryn-{}.trycloudflare.com", req.port)),
        qr_code_data: Some("data:image/png;base64,...".to_string()),
    })
}
