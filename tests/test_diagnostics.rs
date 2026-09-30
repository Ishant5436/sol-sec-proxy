//! Direct tests for the diagnostics feature: robustness against odd input,
//! Anchor log forms, program attribution, and the JSON-RPC error shape.

use serde_json::{json, Value};
use sol_sec_proxy::anchor_logs::{extract_custom_error, parse_anchor_line};
use sol_sec_proxy::config::ProxyConfig;
use sol_sec_proxy::diagnostics::diagnose;
use sol_sec_proxy::interceptor::{InterceptionOutcome, SimulationInterceptor};
use sol_sec_proxy::programs::{
    attribute, last_failure_in_logs, program_id_for_instruction, program_name,
};
use sol_sec_proxy::retry::{classify, RetryClass, INSTRUCTION_TABLE, TRANSACTION_TABLE};
use sol_sec_proxy::rpc_client::SimulationRpcResponse;
use sol_sec_proxy::server::revert_error_json;
use sol_sec_proxy::tx_error::{
    parse_rpc_error, ErrorSource, INSTRUCTION_ERROR_VARIANTS, TRANSACTION_ERROR_VARIANTS,
};
use sol_sec_proxy::wire::{
    CompiledInstruction, MessageHeader, ParsedTransaction, TransactionVersion,
};

const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const SYSTEM: &str = "11111111111111111111111111111111";

fn key(b58: &str) -> [u8; 32] {
    let bytes = bs58::decode(b58).into_vec().expect("base58");
    <[u8; 32]>::try_from(bytes.as_slice()).expect("32 bytes")
}

fn tx_with_programs(program_ids: &[&str]) -> ParsedTransaction {
    let account_keys: Vec<[u8; 32]> = program_ids.iter().map(|p| key(p)).collect();
    let instructions = (0..program_ids.len())
        .map(|i| CompiledInstruction {
            program_id_index: i as u8,
            accounts: vec![],
            data: vec![],
        })
        .collect();
    ParsedTransaction {
        version: TransactionVersion::Legacy,
        signatures: vec![[0u8; 64]],
        header: MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: 0,
        },
        account_keys,
        recent_blockhash: [2u8; 32],
        instructions,
        address_table_lookups: vec![],
        compute_unit_limit: Some(200_000),
        compute_unit_price: Some(1_000),
    }
}

// ---- robustness ------------------------------------------------------------

#[test]
fn u32_max_custom_code_yields_a_diagnosis() {
    let err = json!({"InstructionError": [0, {"Custom": u32::MAX}]});
    let d = diagnose(&err, &[], None);
    assert_eq!(d.kind, "Custom");
    assert_eq!(d.custom_error.code, Some(u32::MAX));
    assert_eq!(d.retry_class, RetryClass::FixInputs);
}

#[test]
fn custom_code_above_u32_is_dropped_not_truncated() {
    let err = json!({"InstructionError": [0, {"Custom": 5_000_000_000u64}]});
    let d = diagnose(&err, &[], None);
    assert_eq!(d.kind, "Custom");
    assert_eq!(d.custom_error.code, None);
}

#[test]
fn unknown_variants_do_not_panic() {
    for err in [
        json!("NotARealError"),
        json!({"NotARealError": 7}),
        json!({"InstructionError": [0, "NotARealInstructionError"]}),
        json!({"InstructionError": [0, {"Whatever": [1, 2]}]}),
    ] {
        let d = diagnose(&err, &[], None);
        assert!(d.kind.starts_with("Unknown("), "{}", d.kind);
        assert_eq!(d.retry_class, RetryClass::Unknown);
    }
}

#[test]
fn malformed_json_shapes_do_not_panic() {
    let shapes = [
        Value::Null,
        json!([]),
        json!([1, 2, 3]),
        json!(42),
        json!(-1.5),
        json!(true),
        json!(""),
        json!({}),
        json!({"a": 1, "b": 2}),
        json!({"InstructionError": null}),
        json!({"InstructionError": []}),
        json!({"InstructionError": [0]}),
        json!({"InstructionError": [0, 1, 2]}),
        json!({"InstructionError": ["x", {"Custom": "y"}]}),
        json!({"InstructionError": [[[]], {"Custom": [[]]}]}),
        json!({"DuplicateInstruction": "one"}),
        json!({"InsufficientFundsForRent": null}),
    ];
    for shape in &shapes {
        let d = diagnose(shape, &[], None);
        assert!(!d.kind.is_empty());
        assert!(!d.suggested_action.is_empty());
    }
}

#[test]
fn deeply_nested_garbage_does_not_panic() {
    let mut value = json!("leaf");
    for _ in 0..100 {
        value = json!({"InstructionError": [0, value]});
    }
    let d = diagnose(&value, &[], None);
    assert!(d.kind.starts_with("Unknown("));
    assert!(d.kind.chars().count() < 200);
}

#[test]
fn logs_longer_than_512_lines_do_not_panic() {
    let mut logs: Vec<String> = (0..5_000)
        .map(|i| format!("Program log: noise {i}"))
        .collect();
    logs.push(format!(
        "Program {SYSTEM} failed: custom program error: 0x1"
    ));
    let err = json!({"InstructionError": [0, {"Custom": 1}]});
    let d = diagnose(&err, &logs, None);
    assert_eq!(d.program_id.as_deref(), Some(SYSTEM));
    assert!(d.logs_snippet.len() <= 5);
}

#[test]
fn very_long_log_lines_are_truncated_in_snippet() {
    let logs = vec![format!("Program log: Error: {}", "x".repeat(10_000))];
    let d = diagnose(&json!("BlockhashNotFound"), &logs, None);
    assert_eq!(d.logs_snippet.len(), 1);
    assert!(d.logs_snippet[0].chars().count() <= 300);
}

#[test]
fn non_ascii_logs_and_errors_do_not_panic() {
    let logs = vec!["Program log: \u{1F980} Error: \u{e9}\u{e9}\u{e9}".to_string()];
    let err = json!("\u{e9}".repeat(500));
    let d = diagnose(&err, &logs, None);
    assert!(d.kind.starts_with("Unknown("));
    assert_eq!(d.logs_snippet.len(), 1);
}

// ---- error shapes ----------------------------------------------------------

#[test]
fn top_level_string_errors_keep_their_names() {
    for name in [
        "BlockhashNotFound",
        "AccountNotFound",
        "InsufficientFundsForFee",
        "AlreadyProcessed",
    ] {
        let p = parse_rpc_error(&json!(name));
        assert_eq!(p.source, ErrorSource::Transaction);
        assert_eq!(p.kind, name);
    }
}

#[test]
fn builtin_instruction_errors_keep_the_instruction_index() {
    for (name, idx) in [
        ("InvalidAccountData", 1u8),
        ("InsufficientFunds", 2),
        ("ComputationalBudgetExceeded", 0),
        ("MissingRequiredSignature", 7),
    ] {
        let p = parse_rpc_error(&json!({"InstructionError": [idx, name]}));
        assert_eq!(p.source, ErrorSource::Instruction);
        assert_eq!(p.kind, name);
        assert_eq!(p.instruction_index, Some(idx));
    }
}

#[test]
fn object_variants_are_parsed() {
    let p = parse_rpc_error(&json!({"InsufficientFundsForRent": {"account_index": 2}}));
    assert_eq!(p.account_index, Some(2));
    let p = parse_rpc_error(&json!({"DuplicateInstruction": 1}));
    assert_eq!(p.instruction_index, Some(1));
    let p = parse_rpc_error(&json!({"InstructionError": [0, {"BorshIoError": "bad"}]}));
    assert_eq!(p.kind, "BorshIoError");
}

#[test]
fn oversized_instruction_index_is_not_truncated() {
    let p = parse_rpc_error(&json!({"InstructionError": [300, "InvalidArgument"]}));
    assert_eq!(p.kind, "InvalidArgument");
    assert_eq!(p.instruction_index, None);
}

// ---- retry table -----------------------------------------------------------

#[test]
fn retry_table_examples_from_the_spec() {
    let t = ErrorSource::Transaction;
    let i = ErrorSource::Instruction;
    assert_eq!(
        classify(t, "BlockhashNotFound").class,
        RetryClass::RebuildWithFreshBlockhash
    );
    assert_eq!(
        classify(t, "AlreadyProcessed").class,
        RetryClass::AlreadyLanded
    );
    assert_eq!(
        classify(t, "InsufficientFundsForFee").class,
        RetryClass::NeedsFunds
    );
    assert_eq!(
        classify(t, "InsufficientFundsForRent").class,
        RetryClass::NeedsFunds
    );
    assert_eq!(classify(t, "AccountNotFound").class, RetryClass::NeedsFunds);
    assert_eq!(
        classify(i, "ComputationalBudgetExceeded").class,
        RetryClass::RaiseComputeLimit
    );
    assert_eq!(classify(i, "Custom").class, RetryClass::FixInputs);
    assert_eq!(
        classify(t, "WouldExceedMaxBlockCostLimit").class,
        RetryClass::RetryLater
    );
    assert_eq!(
        classify(t, "WouldExceedMaxAccountCostLimit").class,
        RetryClass::RetryLater
    );
    assert_eq!(
        classify(t, "WouldExceedMaxVoteCostLimit").class,
        RetryClass::RetryLater
    );
}

#[test]
fn every_known_variant_has_a_nonempty_action() {
    for name in TRANSACTION_ERROR_VARIANTS {
        if *name == "InstructionError" {
            continue;
        }
        assert!(!classify(ErrorSource::Transaction, name).action.is_empty());
    }
    for name in INSTRUCTION_ERROR_VARIANTS {
        assert!(!classify(ErrorSource::Instruction, name).action.is_empty());
    }
    assert!(TRANSACTION_TABLE.len() >= 38 && INSTRUCTION_TABLE.len() >= 50);
}

// ---- Anchor log forms ------------------------------------------------------

#[test]
fn anchor_form_thrown_in() {
    let line = "Program log: AnchorError thrown in programs/x/src/lib.rs:5. Error Code: Boom. Error Number: 6010. Error Message: It blew up.";
    let info = parse_anchor_line(line).expect("anchor line");
    assert_eq!(info.name.as_deref(), Some("Boom"));
    assert_eq!(info.code, Some(6010));
    assert_eq!(info.message.as_deref(), Some("It blew up."));
    assert_eq!(info.account, None);
}

#[test]
fn anchor_form_caused_by_account() {
    let line = "Program log: AnchorError caused by account: authority. Error Code: ConstraintSigner. Error Number: 2002. Error Message: A signer constraint was violated.";
    let info = parse_anchor_line(line).expect("anchor line");
    assert_eq!(info.account.as_deref(), Some("authority"));
    assert_eq!(info.name.as_deref(), Some("ConstraintSigner"));
    assert_eq!(info.code, Some(2002));
}

#[test]
fn anchor_form_plain_hex_reason() {
    let logs = vec![format!(
        "Program {TOKEN} failed: custom program error: 0x1772"
    )];
    let info = extract_custom_error(&logs, None);
    assert_eq!(info.code, Some(6002));
    assert_eq!(info.name, None);
}

#[test]
fn anchor_hex_overflow_is_none() {
    let logs = vec![format!(
        "Program {TOKEN} failed: custom program error: 0xFFFFFFFFFF"
    )];
    assert_eq!(extract_custom_error(&logs, None).code, None);
}

#[test]
fn anchor_line_with_garbage_number_keeps_the_name() {
    let line = "Program log: AnchorError thrown in a.rs:1. Error Code: Weird. Error Number: notanumber. Error Message: hm.";
    let info = parse_anchor_line(line).expect("anchor line");
    assert_eq!(info.name.as_deref(), Some("Weird"));
    assert_eq!(info.code, None);
}

#[test]
fn matching_anchor_line_is_preferred_over_the_last_one() {
    let logs = vec![
        "Program log: AnchorError thrown in a.rs:1. Error Code: First. Error Number: 6001. Error Message: one.".to_string(),
        "Program log: AnchorError thrown in b.rs:2. Error Code: Second. Error Number: 6002. Error Message: two.".to_string(),
    ];
    assert_eq!(
        extract_custom_error(&logs, Some(6001)).name.as_deref(),
        Some("First")
    );
    assert_eq!(
        extract_custom_error(&logs, None).name.as_deref(),
        Some("Second")
    );
}

// ---- program attribution ---------------------------------------------------

#[test]
fn instruction_index_maps_to_program_id() {
    let tx = tx_with_programs(&[SYSTEM, TOKEN]);
    assert_eq!(program_id_for_instruction(&tx, 1).as_deref(), Some(TOKEN));
    assert_eq!(program_id_for_instruction(&tx, 2), None);
    assert_eq!(program_name(TOKEN), Some("SPL Token Program"));
}

#[test]
fn program_key_index_out_of_range_is_none() {
    let mut tx = tx_with_programs(&[SYSTEM]);
    tx.instructions[0].program_id_index = 200;
    assert_eq!(program_id_for_instruction(&tx, 0), None);
}

#[test]
fn last_failed_line_wins_over_inner_failures() {
    let logs = vec![
        format!("Program {SYSTEM} failed: inner"),
        format!("Program {TOKEN} failed: outer"),
    ];
    let f = last_failure_in_logs(&logs).expect("failure line");
    assert_eq!(f.program_id, TOKEN);
    assert_eq!(f.reason, "outer");
}

#[test]
fn attribution_prefers_instruction_and_flags_disagreement() {
    let tx = tx_with_programs(&[SYSTEM, TOKEN]);
    let logs = vec![format!("Program {TOKEN} failed: x")];
    let a = attribute(Some(&tx), Some(0), &logs);
    assert_eq!(a.program_id.as_deref(), Some(SYSTEM));
    assert_eq!(a.sources_agree, Some(false));

    let a = attribute(Some(&tx), Some(1), &logs);
    assert_eq!(a.sources_agree, Some(true));

    let a = attribute(None, None, &logs);
    assert_eq!(a.program_id.as_deref(), Some(TOKEN));
    assert_eq!(a.sources_agree, None);
}

// ---- wiring: interceptor and JSON-RPC error --------------------------------

fn revert_for(
    err: Value,
    logs: Vec<String>,
) -> sol_sec_proxy::interceptor::InterceptionRevertResult {
    let interceptor = SimulationInterceptor::new(ProxyConfig::default());
    let tx = tx_with_programs(&[SYSTEM, TOKEN]);
    let sim = SimulationRpcResponse {
        err: Some(err),
        units_consumed: Some(1_000),
        logs: Some(logs),
    };
    match interceptor.evaluate_simulation(&tx, &sim) {
        InterceptionOutcome::Reverted(rev) => rev,
        InterceptionOutcome::Passed(_) => panic!("expected a revert"),
    }
}

#[test]
fn evaluate_simulation_attaches_diagnosis_for_non_custom_errors() {
    let rev = revert_for(json!("BlockhashNotFound"), vec![]);
    assert_eq!(
        rev.diagnosis.retry_class,
        RetryClass::RebuildWithFreshBlockhash
    );
    assert_eq!(rev.diagnosis.kind, "BlockhashNotFound");
    assert!(rev.avoided_wasted_fee_lamports >= 5_000);
}

#[test]
fn json_rpc_error_data_keeps_legacy_fields_and_adds_diagnosis() {
    let rev = revert_for(
        json!({"InstructionError": [1, {"Custom": 1}]}),
        vec![format!("Program {TOKEN} failed: custom program error: 0x1")],
    );
    let out = revert_error_json(json!(7), &rev);

    assert_eq!(out["id"], json!(7));
    assert_eq!(out["error"]["code"], json!(-32002));
    let data = &out["error"]["data"];
    for legacy in [
        "category",
        "errorName",
        "instructionIndex",
        "avoidedWastedFeeLamports",
        "logs",
    ] {
        assert!(data.get(legacy).is_some(), "missing legacy field {legacy}");
    }

    let diag = &data["diagnosis"];
    assert_eq!(diag["source"], json!("instruction"));
    assert_eq!(diag["kind"], json!("Custom"));
    assert_eq!(diag["instruction_index"], json!(1));
    assert_eq!(diag["program_id"], json!(TOKEN));
    assert_eq!(diag["program_name"], json!("SPL Token Program"));
    assert_eq!(diag["retry_class"], json!("NeedsFunds"));
    assert_eq!(diag["custom_error"]["code"], json!(1));
    assert!(diag["custom_error"].get("account").is_some());
    assert!(diag["suggested_action"].is_string());
    assert!(diag["logs_snippet"].is_array());
}

#[test]
fn json_rpc_message_names_the_error_for_non_custom_failures() {
    let rev = revert_for(json!("BlockhashNotFound"), vec![]);
    let out = revert_error_json(json!(1), &rev);
    let message = out["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("BlockhashNotFound"), "{message}");
}

// ---- property-style sweep --------------------------------------------------

/// Small xorshift generator so the sweep is deterministic and dependency free.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn leaf(rng: &mut Rng) -> Value {
    match rng.pick(9) {
        0 => Value::Null,
        1 => json!(rng.next()),
        2 => json!(rng.next() as i64),
        3 => json!(rng.next() as f64 / 7.0),
        4 => json!(rng.next().is_multiple_of(2)),
        5 => json!("\u{e9}garbage".repeat(rng.pick(50))),
        6 => json!(TRANSACTION_ERROR_VARIANTS[rng.pick(TRANSACTION_ERROR_VARIANTS.len())]),
        7 => json!(INSTRUCTION_ERROR_VARIANTS[rng.pick(INSTRUCTION_ERROR_VARIANTS.len())]),
        _ => json!([]),
    }
}

/// Builds a value iteratively (no recursion) by wrapping a leaf a few times.
fn generate(rng: &mut Rng) -> Value {
    let mut value = leaf(rng);
    for _ in 0..rng.pick(4) {
        value = match rng.pick(6) {
            0 => json!({"InstructionError": [rng.next() % 400, value]}),
            1 => json!({"InstructionError": value}),
            2 => json!({"Custom": value}),
            3 => json!([value, leaf(rng)]),
            4 => json!({"DuplicateInstruction": value}),
            _ => {
                json!({ TRANSACTION_ERROR_VARIANTS[rng.pick(TRANSACTION_ERROR_VARIANTS.len())]: value })
            }
        };
    }
    value
}

#[test]
fn generated_error_shapes_never_panic_and_always_have_a_kind() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let logs = vec![
        format!("Program {SYSTEM} invoke [1]"),
        format!("Program {SYSTEM} failed: custom program error: 0x1"),
    ];
    for _ in 0..600 {
        let err = generate(&mut rng);
        let with_logs = rng.pick(2) == 0;
        let d = diagnose(&err, if with_logs { &logs } else { &[] }, None);
        assert!(!d.kind.is_empty(), "empty kind for {err}");
        assert!(!d.suggested_action.is_empty(), "empty action for {err}");
        let round_trip = serde_json::to_value(&d).expect("diagnosis serialises");
        assert!(round_trip["retry_class"].is_string());
    }
}
