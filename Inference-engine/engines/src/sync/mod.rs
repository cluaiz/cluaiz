use std::net::UdpSocket;
use std::time::Duration;
use std::thread;
use serde::{Serialize, Deserialize};

/// 🧠 Cluaize Sync Identity
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub machine_name: String,
    pub ip_address: String,
    pub last_seen: u64,
}

/// 🛰️ Cluaize P2P Manager
/// Handles device discovery and brain fragment synchronization.
pub struct CluaizeSync {
    discovery_port: u16,
}

impl Default for CluaizeSync {
    fn default() -> Self {
        Self::new()
    }
}

impl CluaizeSync {
    pub fn new() -> Self {
        Self {
            discovery_port: 7711, // Industrial Cluaize Port
        }
    }

    /// 📡 Start Local Discovery (mDNS Alternative)
    /// Broadcasts presence and listens for other Cluaize devices.
    pub fn start_discovery(&self, identity: DeviceIdentity) -> anyhow::Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_broadcast(true)?;
        
        let discovery_msg = serde_json::to_string(&identity)?;
        let broadcast_addr = format!("255.255.255.255:{}", self.discovery_port);

        thread::spawn(move || {
            loop {
                let _ = socket.send_to(discovery_msg.as_bytes(), &broadcast_addr);
                thread::sleep(Duration::from_secs(5)); // Pulse every 5 seconds
            }
        });

        Ok(())
    }

    /// 🎧 Listen for Peers
    pub fn listen_for_peers(&self) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", self.discovery_port))?;
        let mut buf = [0; 1024];

        thread::spawn(move || {
            loop {
                if let Ok((size, addr)) = socket.recv_from(&mut buf) {
                    let msg = String::from_utf8_lossy(&buf[..size]);
                    if let Ok(peer) = serde_json::from_str::<DeviceIdentity>(&msg) {
                        cluaize_shared::dev_info!("🛰️ [P2P] Peer Found: {} at {} ({})", peer.machine_name, addr, peer.device_id);
                        // Future: Add to peer registry and start handshake
                    }
                }
            }
        });

        Ok(())
    }
}
