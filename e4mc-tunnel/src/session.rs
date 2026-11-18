use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, IdleTimeout, RecvStream, TransportConfig, VarInt};
use rustls::RootCertStore;
use rustls_pki_types::CertificateDer;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::broker::RelayInfo;
use crate::protocol::{self, ALPN, ClientboundMessage, ServerboundMessage};

const KEEP_ALIVE: Duration = Duration::from_secs(5);
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub relay: RelayInfo,
    pub lan_address: SocketAddr,
    pub extra_root_certs: Vec<CertificateDer<'static>>,
}

impl SessionConfig {
    pub fn new(relay: RelayInfo, lan_address: SocketAddr) -> Self {
        Self {
            relay,
            lan_address,
            extra_root_certs: Vec::new(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SessionEvent {
    Capabilities(Vec<String>),
    Domain(String),
    Broadcast(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("connecting to relay failed: {0}")]
    Connect(#[from] quinn::ConnectError),
    #[error("connection lost: {0}")]
    Connection(#[from] quinn::ConnectionError),
    #[error("stream closed: {0}")]
    Closed(#[from] quinn::ClosedStream),
    #[error("write failed: {0}")]
    Write(#[from] quinn::WriteError),
    #[error("read failed: {0}")]
    Read(#[from] quinn::ReadExactError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("broken control message: {0}")]
    Protocol(#[from] serde_json::Error),
    #[error("invalid relay address")]
    InvalidAddress,
}

pub fn make_client_config(
    extra_root_certs: &[CertificateDer<'_>],
) -> Result<ClientConfig, std::io::Error> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for cert in extra_root_certs {
        roots
            .add(cert.to_owned())
            .map_err(|error| std::io::Error::other(format!("bad certificate: {error}")))?;
    }
    let mut tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = QuicClientConfig::try_from(tls)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut config = ClientConfig::new(Arc::new(crypto));
    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(KEEP_ALIVE));
    transport.max_idle_timeout(Some(IdleTimeout::from(VarInt::from_u32(
        IDLE_TIMEOUT.as_millis() as u32,
    ))));
    config.transport_config(Arc::new(transport));
    Ok(config)
}

pub async fn run_session(
    config: &SessionConfig,
    events: mpsc::Sender<SessionEvent>,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), SessionError> {
    let client_socket = std::net::UdpSocket::bind(if config.lan_address.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    })?;
    let mut endpoint = quinn::Endpoint::new(
        quinn::EndpointConfig::default(),
        None,
        client_socket,
        Arc::new(quinn::TokioRuntime),
    )?;
    endpoint.set_default_client_config(make_client_config(&config.extra_root_certs)?);

    let addresses: Vec<std::net::SocketAddr> =
        tokio::net::lookup_host((config.relay.host.as_str(), config.relay.port))
            .await
            .map_err(|_| SessionError::InvalidAddress)?
            .collect();
    let address = addresses
        .iter()
        .find(|address| address.is_ipv6() == config.lan_address.is_ipv6())
        .or_else(|| addresses.first())
        .copied()
        .ok_or(SessionError::InvalidAddress)?;
    let connection = endpoint.connect(address, &config.relay.host)?.await?;
    log::info!("relay {} connected", config.relay.id);

    let result = drive(connection.clone(), config, events, shutdown).await;
    connection.close(0u32.into(), b"e4mc-tunnel stopping");
    endpoint.wait_idle().await;
    result
}

async fn drive(
    connection: quinn::Connection,
    config: &SessionConfig,
    events: mpsc::Sender<SessionEvent>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), SessionError> {
    let (mut send_control, mut recv_control) = connection.open_bi().await?;
    send_control
        .write_all(&protocol::encode_serverbound(
            &ServerboundMessage::ProbeCapabilities,
        )?)
        .await?;
    send_control
        .write_all(&protocol::encode_serverbound(
            &ServerboundMessage::RequestDomainAssignment,
        )?)
        .await?;
    let handshake = control_handshake(&mut recv_control, &events);
    tokio::select! {
        result = handshake => { result?; }
        _ = shutdown.changed() => {
            return Ok(());
        }
    }

    log::info!(
        "domain assigned, bridging guest streams to {}",
        config.lan_address
    );
    tokio::spawn(read_control_stream(recv_control, events));
    loop {
        tokio::select! {
            accepted = connection.accept_bi() => {
                log::debug!("guest stream accepted");
                let (send, recv) = accepted?;
                let lan_address = config.lan_address;
                tokio::spawn(async move {
                    if let Err(error) = bridge_stream(send, recv, lan_address).await {
                        log::warn!("guest stream ended with error: {error}");
                    }
                });
            }
            _ = shutdown.changed() => {
                return Ok(());
            }
        }
    }
}

async fn control_handshake(
    recv: &mut RecvStream,
    events: &mpsc::Sender<SessionEvent>,
) -> Result<(), SessionError> {
    loop {
        match read_control_frame(recv).await? {
            ClientboundMessage::HasCapabilities { caps } => {
                log::info!("relay capabilities: {caps:?}");
                let _ = events.send(SessionEvent::Capabilities(caps)).await;
            }
            ClientboundMessage::DomainAssignmentComplete { domain } => {
                log::info!("domain assigned: {domain}");
                let _ = events.send(SessionEvent::Domain(domain)).await;
                return Ok(());
            }
            message => log::warn!("relay sent an early control message: {message:?}"),
        }
    }
}

async fn read_control_stream(
    mut recv: RecvStream,
    events: mpsc::Sender<SessionEvent>,
) -> Result<(), SessionError> {
    loop {
        match read_control_frame(&mut recv).await? {
            ClientboundMessage::UnknownMessage => {
                log::warn!("relay sent an unknown control message")
            }
            ClientboundMessage::TicketRegistered => {
                log::info!("dialtone ticket registered")
            }
            ClientboundMessage::HasCapabilities { caps } => {
                let _ = events.send(SessionEvent::Capabilities(caps)).await;
            }
            ClientboundMessage::DomainAssignmentComplete { domain } => {
                let _ = events.send(SessionEvent::Domain(domain)).await;
            }
            ClientboundMessage::RequestMessageBroadcast { message } => {
                let _ = events.send(SessionEvent::Broadcast(message)).await;
            }
        }
    }
}

async fn read_control_frame(recv: &mut RecvStream) -> Result<ClientboundMessage, SessionError> {
    let mut header = [0u8; 1];
    recv.read_exact(&mut header).await?;
    let length = u32::from(header[0]) as usize;
    let mut frame = vec![0u8; length];
    recv.read_exact(&mut frame).await?;
    log::debug!("relay control frame, {} bytes", frame.len());
    Ok(protocol::decode_clientbound_frame(&frame)?.unwrap_or(ClientboundMessage::UnknownMessage))
}

async fn bridge_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    lan_address: SocketAddr,
) -> Result<(), SessionError> {
    let tcp = TcpStream::connect(lan_address).await?;
    log::debug!("bridged to {lan_address}");
    let (mut tcp_read, mut tcp_write) = tcp.into_split();
    let guest_to_server = tokio::io::copy(&mut recv, &mut tcp_write);
    let server_to_guest = tokio::io::copy(&mut tcp_read, &mut send);
    tokio::try_join!(unwrap_ended(guest_to_server), unwrap_ended(server_to_guest))?;
    tcp_write.shutdown().await?;
    send.finish()?;
    Ok(())
}

async fn unwrap_ended(
    copy: impl std::future::Future<Output = std::io::Result<u64>>,
) -> std::io::Result<u64> {
    match copy.await {
        Ok(count) => Ok(count),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(0),
        Err(error) => Err(error),
    }
}
