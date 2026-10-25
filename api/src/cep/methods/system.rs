//! `system.*`: the Engine and the machine it runs on (03-method-catalog Part B).
//! Owner: Claude until `system.info` is in, then Wave B (05-chatgpt-plan.md §2).

use crate::cep::CallCtx;
use cluaiz_cep::{Call, Meta, NoParams, Registry, Scope};
use cluaiz_hw::HardwareProber;
use serde_json::{json, Value};

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "system.info",
        Meta::new(Scope::Read, "Engine name and version, CEP version, hardware, and the LAN address").example(
            json!({}),
            json!({ "engine": "Cluaiz Engine", "version": "0.1.0", "cep_version": 1,
                    "hardware": { "cpu": {}, "memory": {}, "gpus": [] }, "network": { "lan_ip": "192.168.1.20" } }),
        ),
        |_: CallCtx, _: Call, _: NoParams| async move { Ok(info()) },
    );
}

fn info() -> Value {
    json!({
        "engine": "Cluaiz Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "cep_version": cluaiz_cep::discover::CEP_VERSION,
        "hardware": HardwareProber::current(),
        "network": { "lan_ip": lan_ip() },
    })
}

/// The address other devices on the network reach this machine at: the local side of a UDP
/// socket pointed at a public address (connect sends no packet)
pub fn lan_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}
