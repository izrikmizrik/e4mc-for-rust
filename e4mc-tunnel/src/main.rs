use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser;
use rustls_pki_types::pem::PemObject;
use tokio::sync::{mpsc, watch};

use e4mc_tunnel::{
    DEFAULT_BROKER_URL, RelayInfo, SessionConfig, SessionEvent, resolve_relay, run_session,
};

#[derive(Parser, Debug)]
#[command(
    name = "e4mc-tunnel",
    version,
    about = "Open a Horizon (Rust Minecraft) LAN world to the internet through the public e4mc relay network",
    after_help = "The LAN port is what Horizon prints when the world is opened to LAN. \
        Friends join with the assigned <words>.e4mc.link domain, Direct Connect, no mods needed."
)]
struct Args {
    #[arg(
        short,
        long,
        help = "TCP port of the Horizon LAN world on this machine (127.0.0.1)"
    )]
    port: u16,
    #[arg(
        long,
        help = "Use this relay as host:port instead of asking the e4mc broker"
    )]
    custom_relay: Option<String>,
    #[arg(
        long,
        default_value = DEFAULT_BROKER_URL,
        help = "Broker URL returning the best relay as {id, host, port} JSON"
    )]
    broker_url: String,
    #[arg(
        long,
        help = "PEM file with extra root certificates for a private relay"
    )]
    extra_certs: Option<String>,
    #[arg(
        long,
        default_value = "1",
        help = "Backoff base in seconds between reconnect attempts"
    )]
    backoff_seconds: u64,
}

fn relay_from_spec(spec: &str) -> anyhow::Result<RelayInfo> {
    let (host, port) = spec
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("–-custom-relay must be host:port, got {spec}"))?;
    Ok(RelayInfo {
        id: "custom".to_string(),
        host: host.to_string(),
        port: port.parse()?,
    })
}

fn event_printer(mut events: mpsc::Receiver<SessionEvent>) {
    while let Some(event) = events.blocking_recv() {
        match event {
            SessionEvent::Domain(domain) => {
                println!();
                println!("  Your world is open to the internet:");
                println!("    {domain}");
                println!();
                println!("  Friends connect with this domain, Direct Connect, no mods.");
                println!("  Ctrl+C closes the tunnel.");
            }
            SessionEvent::Broadcast(message) => println!("[relay] {message}"),
            SessionEvent::Capabilities(caps) => log::debug!("relay capabilities: {caps:?}"),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let lan_address = SocketAddr::from(([127, 0, 0, 1], args.port));
    let relay = match &args.custom_relay {
        Some(spec) => relay_from_spec(spec)?,
        None => resolve_relay(Some(&args.broker_url))?,
    };
    log::info!("using relay {}", relay.id);

    let mut config = SessionConfig::new(relay, lan_address);
    if let Some(path) = &args.extra_certs {
        config.extra_root_certs = rustls_pki_types::CertificateDer::pem_file_iter(path)?
            .collect::<Result<Vec<_>, _>>()?;
    }

    let (events_tx, events_rx) = mpsc::channel::<SessionEvent>(16);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let printer = std::thread::spawn(move || event_printer(events_rx));
    let backoff = Duration::from_secs(args.backoff_seconds.max(1));
    let mut pause = Duration::from_secs(1);

    let result = loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("shutdown requested");
                let _ = shutdown_tx.send(true);
                break Ok(());
            }

            outcome = run_session(&config, events_tx.clone(), shutdown_rx.clone()) => {
                match outcome {
                    Ok(()) => break Ok(()),
                    Err(error) => {
                        log::error!("session failed: {error}; retrying in {pause:?}");
                        tokio::time::sleep(pause).await;
                        pause = pause.saturating_mul(2).min(Duration::from_secs(60)).max(backoff);
                        continue;
                    }
                }
            }
        }
    };

    let _ = shutdown_tx.send(true);
    let _ = printer.join();
    result
}
