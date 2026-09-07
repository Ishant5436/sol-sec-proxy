use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SolanaRpcClient {
    client: reqwest::Client,
    upstream_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationRpcResponse {
    pub err: Option<serde_json::Value>,
    #[serde(rename = "unitsConsumed")]
    pub units_consumed: Option<u64>,
    pub logs: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum RpcError {
    NetworkError(String),
    SerializationError(String),
    JsonRpcError { code: i64, message: String },
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NetworkError(s) => write!(f, "RPC Network Error: {}", s),
            Self::SerializationError(s) => write!(f, "RPC Serialization Error: {}", s),
            Self::JsonRpcError { code, message } => {
                write!(f, "Solana JSON-RPC Error ({}): {}", code, message)
            }
        }
    }
}

impl SolanaRpcClient {
    pub fn new(upstream_url: &str, timeout_ms: u64) -> Self {
        assert!(!upstream_url.is_empty(), "Upstream URL cannot be empty");
        assert!(timeout_ms > 0, "Timeout must be greater than zero");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            upstream_url: upstream_url.to_string(),
        }
    }

    pub async fn simulate_transaction(
        &self,
        encoded_tx: &str,
        sig_verify: bool,
        replace_recent_blockhash: bool,
    ) -> Result<SimulationRpcResponse, RpcError> {
        assert!(!encoded_tx.is_empty(), "Encoded tx cannot be empty");
        assert!(encoded_tx.len() <= 4096, "Encoded tx size upper bound");

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": [
                encoded_tx,
                {
                    "sigVerify": sig_verify,
                    "replaceRecentBlockhash": replace_recent_blockhash,
                    "encoding": "base64",
                    "commitment": "confirmed"
                }
            ]
        });

        let resp_val = self.forward_raw_json_rpc(&payload).await?;
        if let Some(err_obj) = resp_val.get("error") {
            let code = err_obj
                .get("code")
                .and_then(|c| c.as_i64())
                .unwrap_or(-32000);
            let msg = err_obj
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown RPC error");
            return Err(RpcError::JsonRpcError {
                code,
                message: msg.to_string(),
            });
        }

        let val_obj = resp_val
            .get("result")
            .and_then(|r| r.get("value"))
            .ok_or_else(|| {
                RpcError::SerializationError(
                    "Missing result.value in simulate response".to_string(),
                )
            })?;

        let sim_resp: SimulationRpcResponse = serde_json::from_value(val_obj.clone())
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;

        Ok(sim_resp)
    }

    pub async fn forward_raw_json_rpc(
        &self,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, RpcError> {
        assert!(!self.upstream_url.is_empty(), "Upstream URL invariant");
        assert!(payload.is_object(), "Payload must be a JSON object");

        let resp = self
            .client
            .post(&self.upstream_url)
            .json(payload)
            .send()
            .await
            .map_err(|e| RpcError::NetworkError(e.to_string()))?;

        let json_val = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;

        Ok(json_val)
    }
}
