//! Fixture-driven diagnostics tests. Each JSON file under tests/fixtures holds
//! a realistic `simulateTransaction` `err` and `logs`, an optional compiled
//! transaction, and the expected diagnosis fields. Offline only.

use serde_json::Value;
use sol_sec_proxy::diagnostics::{diagnose, Diagnosis};
use sol_sec_proxy::wire::{
    CompiledInstruction, MessageHeader, ParsedTransaction, TransactionVersion,
};

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn load(name: &str) -> Value {
    let path = format!("{}/{}.json", FIXTURE_DIR, name);
    let text = std::fs::read_to_string(&path).expect("fixture file is readable");
    serde_json::from_str(&text).expect("fixture is valid JSON")
}

fn build_tx(spec: &Value) -> ParsedTransaction {
    let keys: Vec<[u8; 32]> = spec["account_keys"]
        .as_array()
        .expect("account_keys array")
        .iter()
        .map(|k| {
            let bytes = bs58::decode(k.as_str().expect("key is a string"))
                .into_vec()
                .expect("key is base58");
            <[u8; 32]>::try_from(bytes.as_slice()).expect("key is 32 bytes")
        })
        .collect();
    let instructions = spec["instruction_program_indexes"]
        .as_array()
        .expect("instruction_program_indexes array")
        .iter()
        .map(|i| CompiledInstruction {
            program_id_index: i.as_u64().expect("index") as u8,
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
        account_keys: keys,
        recent_blockhash: [2u8; 32],
        instructions,
        address_table_lookups: vec![],
        compute_unit_limit: None,
        compute_unit_price: None,
    }
}

fn run(fixture: &Value) -> Diagnosis {
    let logs: Vec<String> = fixture["logs"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let tx = fixture.get("tx").map(build_tx);
    diagnose(&fixture["err"], &logs, tx.as_ref())
}

fn expect_opt_str(expected: &Value, key: &str, actual: Option<&str>) {
    if let Some(want) = expected.get(key) {
        assert_eq!(want.as_str(), actual, "field {}", key);
    }
}

fn check(name: &str, fixture: &Value, d: &Diagnosis) {
    let e = &fixture["expect"];
    let as_json = serde_json::to_value(d).expect("diagnosis serialises");

    if let Some(want) = e.get("source") {
        assert_eq!(want, &as_json["source"], "{}: source", name);
    }
    if let Some(want) = e.get("kind") {
        assert_eq!(want.as_str(), Some(d.kind.as_str()), "{}: kind", name);
    }
    if let Some(prefix) = e.get("kind_prefix").and_then(|p| p.as_str()) {
        assert!(d.kind.starts_with(prefix), "{}: kind {}", name, d.kind);
    }
    if let Some(want) = e.get("retry_class") {
        assert_eq!(want, &as_json["retry_class"], "{}: retry_class", name);
    }
    if let Some(want) = e.get("instruction_index") {
        let got = d.instruction_index.map(u64::from);
        assert_eq!(want.as_u64(), got, "{}: instruction_index", name);
    }
    expect_opt_str(e, "program_id", d.program_id.as_deref());
    expect_opt_str(e, "program_name", d.program_name.as_deref());
    expect_opt_str(e, "custom_name", d.custom_error.name.as_deref());
    expect_opt_str(e, "custom_message", d.custom_error.message.as_deref());
    expect_opt_str(e, "custom_account", d.custom_error.account.as_deref());
    if let Some(want) = e.get("custom_code") {
        assert_eq!(
            want.as_u64(),
            d.custom_error.code.map(u64::from),
            "{}",
            name
        );
    }
    if let Some(needle) = e.get("action_contains").and_then(|n| n.as_str()) {
        assert!(
            d.suggested_action.contains(needle),
            "{}: {}",
            name,
            d.suggested_action
        );
    }
    assert!(!d.kind.is_empty(), "{}: kind is never empty", name);
    assert!(
        !d.suggested_action.is_empty(),
        "{}: action is never empty",
        name
    );
}

fn run_fixture(name: &str) {
    let fixture = load(name);
    let diagnosis = run(&fixture);
    check(name, &fixture, &diagnosis);
}

macro_rules! fixture_tests {
    ($($name:ident),* $(,)?) => {
        const FIXTURE_NAMES: &[&str] = &[$(stringify!($name)),*];
        $(
            #[test]
            fn $name() {
                run_fixture(stringify!($name));
            }
        )*
    };
}

fixture_tests!(
    account_in_use,
    account_not_found,
    address_lookup_table_not_found,
    already_processed,
    anchor_caused_by_account,
    anchor_thrown_in_slippage,
    blockhash_not_found,
    cpi_inner_anchor_error,
    custom_u32_max,
    duplicate_instruction,
    instruction_index_out_of_range,
    insufficient_funds_for_fee,
    insufficient_funds_for_rent,
    ix_arithmetic_overflow,
    ix_borsh_io_error_object,
    ix_compute_budget_exceeded,
    ix_insufficient_funds,
    ix_invalid_account_data,
    ix_missing_required_signature,
    ix_program_failed_to_complete,
    log_program_disagrees_with_instruction,
    plain_custom_hex_only,
    program_account_not_found,
    program_execution_temporarily_restricted,
    signature_failure,
    system_transfer_insufficient_lamports,
    token_transfer_insufficient_funds,
    too_many_account_locks,
    unknown_instruction_variant,
    unknown_transaction_variant,
    would_exceed_max_block_cost_limit,
);

#[test]
fn every_fixture_file_has_a_test() {
    let mut on_disk: Vec<String> = std::fs::read_dir(FIXTURE_DIR)
        .expect("fixture dir exists")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".json").map(str::to_string))
        .collect();
    on_disk.sort();

    let mut listed: Vec<String> = FIXTURE_NAMES.iter().map(|s| s.to_string()).collect();
    listed.sort();

    assert!(on_disk.len() >= 20, "at least 20 distinct fixtures");
    assert_eq!(on_disk, listed);
}

#[test]
fn fixtures_cover_every_retry_class() {
    let wanted = [
        "RebuildWithFreshBlockhash",
        "AlreadyLanded",
        "NeedsFunds",
        "RaiseComputeLimit",
        "FixInputs",
        "RetryLater",
        "Fatal",
        "Unknown",
    ];
    let mut seen = std::collections::BTreeSet::new();
    for name in FIXTURE_NAMES {
        let fixture = load(name);
        let d = run(&fixture);
        let value = serde_json::to_value(d.retry_class).expect("serialises");
        seen.insert(value.as_str().unwrap_or_default().to_string());
    }
    for class in wanted {
        assert!(seen.contains(class), "no fixture produces {}", class);
    }
}
