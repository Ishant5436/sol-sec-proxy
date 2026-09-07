use sol_sec_proxy::config::ProxyConfig;
use sol_sec_proxy::interceptor::{
    extract_instruction_error_code, InterceptionOutcome, SimulationInterceptor,
};
use sol_sec_proxy::rpc_client::SimulationRpcResponse;
use sol_sec_proxy::wire::{MessageHeader, ParsedTransaction, TransactionVersion};

#[test]
fn test_extract_instruction_error_code() {
    let err_val = serde_json::json!({
        "InstructionError": [2, { "Custom": 6001 }]
    });

    let (code, idx) = extract_instruction_error_code(&err_val);
    assert_eq!(code, 6001);
    assert_eq!(idx, Some(2));
}

#[test]
fn test_evaluate_simulation_revert() {
    let cfg = ProxyConfig::default();
    let interceptor = SimulationInterceptor::new(cfg);

    let tx = ParsedTransaction {
        version: TransactionVersion::Legacy,
        signatures: vec![[0u8; 64]],
        header: MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: 0,
        },
        account_keys: vec![[1u8; 32]],
        recent_blockhash: [2u8; 32],
        instructions: vec![],
        address_table_lookups: vec![],
        compute_unit_limit: Some(200_000),
        compute_unit_price: Some(10_000), // 10,000 micro-lamports
    };

    let sim = SimulationRpcResponse {
        err: Some(serde_json::json!({
            "InstructionError": [0, { "Custom": 2003 }]
        })),
        units_consumed: Some(12_000),
        logs: Some(vec![
            "Program log: AnchorError thrown in programs/vault/src/lib.rs:42. Error Code: ConstraintRaw. Error Number: 2003. Error Message: A raw constraint was violated.".to_string(),
        ]),
    };

    let outcome = interceptor.evaluate_simulation(&tx, &sim);
    match outcome {
        InterceptionOutcome::Reverted(rev) => {
            assert!(!rev.is_valid);
            assert_eq!(rev.decoded_error.error_code, 2003);
            assert_eq!(rev.decoded_error.error_name, "ConstraintRaw");
            assert_eq!(rev.decoded_error.message, "A raw constraint was violated.");
            assert!(rev.avoided_wasted_fee_lamports > 5_000);
        }
        _ => panic!("Expected Reverted outcome"),
    }
}

#[test]
fn test_evaluate_simulation_passed() {
    let cfg = ProxyConfig::default();
    let interceptor = SimulationInterceptor::new(cfg);

    let tx = ParsedTransaction {
        version: TransactionVersion::Legacy,
        signatures: vec![[0u8; 64]],
        header: MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: 0,
        },
        account_keys: vec![[1u8; 32]],
        recent_blockhash: [2u8; 32],
        instructions: vec![],
        address_table_lookups: vec![],
        compute_unit_limit: Some(500_000),
        compute_unit_price: Some(1_000),
    };

    let sim = SimulationRpcResponse {
        err: None,
        units_consumed: Some(40_000),
        logs: Some(vec!["Program log: Success".to_string()]),
    };

    let outcome = interceptor.evaluate_simulation(&tx, &sim);
    match outcome {
        InterceptionOutcome::Passed(pass) => {
            assert!(pass.is_valid);
            assert_eq!(pass.recommendation.units_consumed, 40_000);
            assert_eq!(pass.recommendation.recommended_cu, 46_000); // 40k + 15% = 46k
            assert_eq!(pass.recommendation.potential_cu_saved, 454_000);
        }
        _ => panic!("Expected Passed outcome"),
    }
}
