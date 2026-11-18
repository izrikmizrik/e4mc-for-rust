use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RelayInfo {
    pub id: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("broker response was not valid JSON: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("io error talking to the broker: {0}")]
    Io(#[from] std::io::Error),
    #[error("broker returned status {status}")]
    Status { status: u16 },
    #[error("HTTP request to broker failed: {0}")]
    Http(Box<ureq::Error>),
}

impl From<ureq::Error> for BrokerError {
    fn from(value: ureq::Error) -> Self {
        Self::Http(Box::new(value))
    }
}

pub const DEFAULT_BROKER_URL: &str = "https://broker.e4mc.link/getBestRelay";
pub const DEFAULT_RELAY_HOST: &str = "test.e4mc.link";
pub const DEFAULT_RELAY_PORT: u16 = 25575;

pub fn resolve_relay(broker_url: Option<&str>) -> Result<RelayInfo, BrokerError> {
    let Some(broker_url) = broker_url else {
        return Ok(RelayInfo {
            id: "custom".to_string(),
            host: DEFAULT_RELAY_HOST.to_string(),
            port: DEFAULT_RELAY_PORT,
        });
    };
    let response = ureq::get(broker_url)
        .set("Accept", "application/json")
        .call()?;
    let status = response.status();
    if status != 200 {
        return Err(BrokerError::Status { status });
    }
    Ok(serde_json::from_str(&response.into_string()?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_response_shape_matches_java_mod() {
        let info: RelayInfo =
            serde_json::from_str(r#"{"id":"eur-01","host":"eur-01.e4mc.link","port":25575}"#)
                .unwrap();
        assert_eq!(info.host, "eur-01.e4mc.link");
        assert_eq!(info.port, 25575);
    }
}
