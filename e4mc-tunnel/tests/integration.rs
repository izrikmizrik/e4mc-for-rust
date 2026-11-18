use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::QuicServerConfig;
use quinn::{ServerConfig as QuinnServerConfig, TransportConfig};
use rcgen::generate_simple_self_signed;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

fn init_log() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        env_logger::builder().is_test(true).try_init().ok();
    });
}

use e4mc_tunnel::{
    ClientboundMessage, RelayInfo, ServerboundMessage, SessionConfig, SessionEvent,
    encode_serverbound, run_session,
};

const GUEST_GREETING: &[u8] = b"e4mc-tunnel test payload";
const TEST_DOMAIN: &str = "brown-fox.test.link";
const WAIT: Duration = Duration::from_secs(15);

async fn spawn_echo_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0u8; 256];
                loop {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            if socket.write_all(&buffer[..read]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    address
}

async fn serve_fake_relay(
    endpoint: quinn::Endpoint,
    round_trip_done: tokio::sync::mpsc::Sender<()>,
) {
    let connection = endpoint
        .accept()
        .await
        .expect("host connects")
        .await
        .expect("handshake completes");

    let (mut send, mut recv) = connection.accept_bi().await.expect("control stream opens");

    let first = read_frame(&mut recv).await;
    assert_eq!(
        first,
        ServerboundMessage::ProbeCapabilities,
        "first control message asks the relay for its capabilities"
    );
    let second = read_frame(&mut recv).await;
    assert_eq!(
        second,
        ServerboundMessage::RequestDomainAssignment,
        "second control message requests the domain assignment"
    );

    let has_caps = serde_json::to_vec(&ClientboundMessage::HasCapabilities {
        caps: vec!["fake_relay".to_string()],
    })
    .unwrap();
    let mut reply = vec![has_caps.len() as u8];
    reply.extend_from_slice(&has_caps);
    send.write_all(&reply).await.unwrap();

    let domain_assigned = serde_json::to_vec(&ClientboundMessage::DomainAssignmentComplete {
        domain: TEST_DOMAIN.to_string(),
    })
    .unwrap();
    let mut reply = vec![domain_assigned.len() as u8];
    reply.extend_from_slice(&domain_assigned);
    send.write_all(&reply).await.unwrap();

    let (mut guest_send, mut guest_recv) = connection.open_bi().await.expect("guest stream opens");
    guest_send.write_all(GUEST_GREETING).await.unwrap();
    guest_send.finish().unwrap();
    let mut echoed = vec![0u8; GUEST_GREETING.len()];
    tokio::time::timeout(WAIT, guest_recv.read_exact(&mut echoed))
        .await
        .expect("echo arrives in time")
        .expect("echo stream survives the LAN round trip");
    assert_eq!(echoed, GUEST_GREETING, "guest bytes reach the LAN port");

    let _ = round_trip_done.send(()).await;
    let _ = connection.closed().await;
}

async fn read_frame(recv: &mut quinn::RecvStream) -> ServerboundMessage {
    let mut header = [0u8; 1];
    recv.read_exact(&mut header).await.unwrap();
    let mut frame = vec![0u8; usize::from(header[0])];
    recv.read_exact(&mut frame).await.unwrap();
    serde_json::from_slice(&frame).expect("valid JSON control frame")
}

fn test_config(
    relay_address: std::net::SocketAddr,
    lan_address: std::net::SocketAddr,
) -> SessionConfig {
    SessionConfig::new(
        RelayInfo {
            id: "fake".into(),
            host: "localhost".into(),
            port: relay_address.port(),
        },
        lan_address,
    )
}

async fn make_relay_endpoint() -> (
    quinn::Endpoint,
    Option<rustls_pki_types::CertificateDer<'static>>,
) {
    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let certificate = CertificateDer::from(cert.cert);
    let key = PrivateKeyDer::try_from(cert.key_pair.serialize_der()).unwrap();
    let mut demo = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.clone()], key.clone_key())
        .unwrap();
    demo.alpn_protocols = vec![e4mc_tunnel::ALPN.to_vec()];
    let crypto = QuicServerConfig::try_from(demo).unwrap();
    let mut server_config = QuinnServerConfig::with_crypto(Arc::new(crypto));
    let mut transport = TransportConfig::default();
    transport.max_concurrent_bidi_streams(256u32.into());
    transport.max_concurrent_uni_streams(0u32.into());
    server_config.transport_config(Arc::new(transport));
    let endpoint = quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    (endpoint, Some(certificate))
}

#[tokio::test(flavor = "multi_thread")]
async fn session_assigns_a_domain_and_bridges_guest_traffic_to_the_lan_port() {
    init_log();
    let echo_address = spawn_echo_server().await;
    let (relay_endpoint, certificate) = make_relay_endpoint().await;
    let relay_address = relay_endpoint.local_addr().unwrap();

    let (round_trip_done, mut round_trip_done_rx) = tokio::sync::mpsc::channel::<()>(1);
    let relay_task = tokio::spawn(serve_fake_relay(relay_endpoint, round_trip_done));

    let config = test_config(relay_address, echo_address);
    let mut config = config;
    config.extra_root_certs = certificate.into_iter().collect();

    let (events_tx, mut events_rx) = mpsc::channel::<SessionEvent>(16);
    let (shutdown, shutdown_rx) = watch::channel(false);
    let mut runner =
        tokio::spawn(async move { run_session(&config, events_tx, shutdown_rx).await });

    let mut domain: Option<String> = None;
    let mut session_result: Option<Result<(), e4mc_tunnel::SessionError>> = None;
    loop {
        tokio::select! {
            maybe_event = tokio::time::timeout(WAIT, events_rx.recv()) => {
                match maybe_event {
                    Err(_) => panic!("no session events within {WAIT:?}"),
                    Ok(None) => break,
                    Ok(Some(SessionEvent::Domain(assigned))) => {
                        domain = Some(assigned);
                        break;
                    }
                    Ok(Some(_)) => {}
                }
            }
            finished = &mut runner => {
                session_result = Some(finished.expect("runner does not panic"));
                break;
            }
        }
    }

    assert!(
        session_result.is_none(),
        "session failed early: {session_result:?}"
    );
    assert_eq!(domain.as_deref(), Some(TEST_DOMAIN));

    tokio::time::timeout(WAIT, round_trip_done_rx.recv())
        .await
        .expect("fake relay bridges a guest stream in time")
        .expect("fake relay signals the finished guest round trip");

    let _ = shutdown.send(true);
    let result = tokio::time::timeout(WAIT, &mut runner)
        .await
        .expect("session stops on shutdown")
        .expect("runner does not panic");
    assert!(result.is_ok(), "session ends cleanly: {result:?}");

    relay_task.await.expect("fake relay does not panic");
}

#[tokio::test]
async fn serverbound_encoding_is_self_delimiting_json() {
    let encoded = encode_serverbound(&ServerboundMessage::ProbeCapabilities).unwrap();
    assert_eq!(encoded[0] as usize + 1, encoded.len());
    let (length, header) = e4mc_tunnel::read_varint(&encoded).unwrap();
    assert_eq!(header, 1);
    let decoded: ServerboundMessage =
        serde_json::from_slice(&encoded[1..1 + length as usize]).unwrap();
    assert_eq!(decoded, ServerboundMessage::ProbeCapabilities);
}
