use crate::config::ProxyConfig;
use crate::cu_optimizer::{
    analyze_account_write_locks, calculate_optimal_compute_units, AccountLockAnalysis,
    CuOptimizationRecommendation,
};
use crate::error_decoder::{decode_simulation_error, DecodedError};
use crate::rpc_client::{RpcError, SimulationRpcResponse, SolanaRpcClient};
use crate::wire::{decode_transaction_from_wire_string, ParsedTransaction};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptionPassResult {
    pub is_valid: bool,
    pub recommendation: CuOptimizationRecommendation,
    pub locks_analysis: AccountLockAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptionRevertResult {
    pub is_valid: bool,
    pub decoded_error: DecodedError,
    pub avoided_wasted_fee_lamports: u64,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InterceptionOutcome {
    Passed(InterceptionPassResult),
    Reverted(InterceptionRevertResult),
}

pub struct SimulationInterceptor {
    config: ProxyConfig,
    rpc_client: SolanaRpcClient,
}

impl SimulationInterceptor {
    pub fn new(config: ProxyConfig) -> Self {
        assert!(!config.upstream_rpc_url.is_empty(), "RPC URL invariant");
        assert!(config.timeout_ms > 0, "Timeout invariant");

        let rpc_client = SolanaRpcClient::new(&config.upstream_rpc_url, config.timeout_ms);
        Self { config, rpc_client }
    }

    pub async fn process_transaction(
        &self,
        encoded_tx: &str,
    ) -> Result<(ParsedTransaction, InterceptionOutcome), RpcError> {
        assert!(!encoded_tx.is_empty(), "Encoded tx cannot be empty");
        assert!(encoded_tx.len() <= 4096, "Encoded tx size invariant");

        let parsed_tx = decode_transaction_from_wire_string(encoded_tx)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;

        let sim_resp = self
            .rpc_client
            .simulate_transaction(encoded_tx, false, true)
            .await?;

        let outcome = self.evaluate_simulation(&parsed_tx, &sim_resp);
        Ok((parsed_tx, outcome))
    }

    pub fn evaluate_simulation(
        &self,
        tx: &ParsedTransaction,
        sim: &SimulationRpcResponse,
    ) -> InterceptionOutcome {
        assert!(self.config.cu_safety_buffer_percent <= 100, "Buffer bound");
        assert!(self.config.max_cu_limit <= 1_400_000, "Max limit bound");

        let logs = sim.logs.clone().unwrap_or_default();

        if let Some(err_val) = &sim.err {
            let (code, ix_idx) = extract_instruction_error_code(err_val);
            let decoded = decode_simulation_error(ix_idx, code, &logs);

            let priority_fee = tx.compute_unit_price.unwrap_or(0);
            let requested_cu = tx.compute_unit_limit.unwrap_or(200_000);
            let avoided_fee = ((requested_cu as u64) * priority_fee) / 1_000_000 + 5_000;

            InterceptionOutcome::Reverted(InterceptionRevertResult {
                is_valid: false,
                decoded_error: decoded,
                avoided_wasted_fee_lamports: avoided_fee,
                logs,
            })
        } else {
            let units = sim.units_consumed.unwrap_or(5_000);
            let rec = calculate_optimal_compute_units(
                units,
                tx.compute_unit_limit,
                self.config.cu_safety_buffer_percent,
                self.config.max_cu_limit,
                tx.compute_unit_price.unwrap_or(0),
            );

            let locks = analyze_account_write_locks(
                &tx.account_keys,
                tx.header.num_required_signatures,
                tx.header.num_readonly_signed_accounts,
                tx.header.num_readonly_unsigned_accounts,
            );

            InterceptionOutcome::Passed(InterceptionPassResult {
                is_valid: true,
                recommendation: rec,
                locks_analysis: locks,
            })
        }
    }
}

pub fn extract_instruction_error_code(err_val: &serde_json::Value) -> (u32, Option<u8>) {
    assert!(!err_val.is_null(), "Error value cannot be null");
    assert!(
        err_val.is_object() || err_val.is_string(),
        "Error structure invariant"
    );

    if let Some(ix_err) = err_val.get("InstructionError") {
        if let Some(arr) = ix_err.as_array() {
            let idx = arr.first().and_then(|v| v.as_u64()).map(|u| u as u8);
            if let Some(custom_obj) = arr.get(1).and_then(|v| v.get("Custom")) {
                let code = custom_obj.as_u64().unwrap_or(0) as u32;
                return (code, idx);
            }
        }
    }
    (0, None)
}
