use serde::{Deserialize, Serialize};

pub const ALPN: &[u8] = b"quiclime";

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "kind")]
#[serde(rename_all = "snake_case")]
pub enum ServerboundMessage {
    ProbeCapabilities,
    RequestDomainAssignment,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "kind")]
#[serde(rename_all = "snake_case")]
pub enum ClientboundMessage {
    UnknownMessage,
    HasCapabilities { caps: Vec<String> },
    DomainAssignmentComplete { domain: String },
    RequestMessageBroadcast { message: String },
    TicketRegistered,
}

pub fn write_varint(buf: &mut Vec<u8>, mut value: u32) {
    while value & 0xffffff80 != 0 {
        buf.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

pub fn read_varint(input: &[u8]) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    for index in 0..5u32 {
        let byte = *input.get(index as usize)?;
        result |= u32::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Some((result, index as usize + 1));
        }
    }
    None
}

pub fn encode_serverbound(message: &ServerboundMessage) -> serde_json::Result<Vec<u8>> {
    let json = serde_json::to_vec(message)?;
    let mut out = Vec::with_capacity(json.len() + 2);
    write_varint(&mut out, json.len() as u32);
    out.extend_from_slice(&json);
    Ok(out)
}

pub fn decode_clientbound_frame(frame: &[u8]) -> serde_json::Result<Option<ClientboundMessage>> {
    if frame.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(frame)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        for value in [0u32, 1, 127, 128, 255, 16383, 16384, u32::MAX] {
            let mut buf = Vec::new();
            write_varint(&mut buf, value);
            let (decoded, len) = read_varint(&buf).expect("varint decodes");
            assert_eq!(decoded, value);
            assert_eq!(len, buf.len());
        }
    }

    #[test]
    fn serverbound_encoding_matches_java_mod() {
        let encoded = encode_serverbound(&ServerboundMessage::ProbeCapabilities).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&encoded[1..]),
            r#"{"kind":"probe_capabilities"}"#
        );
        assert_eq!(encoded[0], encoded.len() as u8 - 1);
    }

    #[test]
    fn clientbound_decoding_matches_relay_format() {
        let payload = br#"{"kind":"domain_assignment_complete","domain":"brown-fox.eu.e4mc.link"}"#;
        let mut framed = vec![payload.len() as u8];
        framed.extend_from_slice(payload);
        let (length, header) = read_varint(&framed).unwrap();
        assert_eq!(header, 1);
        let message = decode_clientbound_frame(&framed[1..1 + length as usize])
            .unwrap()
            .unwrap();
        assert_eq!(
            message,
            ClientboundMessage::DomainAssignmentComplete {
                domain: "brown-fox.eu.e4mc.link".to_string()
            }
        );
    }
}
