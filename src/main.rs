#![deny(warnings)]

use sol_sec_proxy::config::ProxyConfig;
use sol_sec_proxy::server::ProxyServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("============================================================");
    println!(" sol-sec-proxy: Solana Pre-Execution Transaction Firewall");
    println!(" Deterministic Safety-Critical Standards (Holzmann Invariants)");
    println!("============================================================");

    let config = ProxyConfig::from_env().unwrap_or_else(|err| {
        eprintln!("Configuration error: {}", err);
        std::process::exit(1);
    });

    assert!(config.port > 0, "Configuration port invariant");
    assert!(
        !config.upstream_rpc_url.is_empty(),
        "Configuration upstream invariant"
    );

    let server = ProxyServer::new(config);
    server.run().await
}
