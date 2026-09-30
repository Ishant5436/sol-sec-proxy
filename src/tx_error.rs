//! Model of the JSON shapes an RPC node uses for `TransactionError` and
//! `InstructionError` in `simulateTransaction` responses.
//!
//! Variant names are taken from the `solana-transaction-error` and
//! `solana-instruction-error` crates (anza-xyz/solana-sdk, checked 2026-09-30).
//! Serde's external tagging gives four wire shapes, all handled here:
//!
//! * unit variant            -> `"BlockhashNotFound"`
//! * newtype variant         -> `{"DuplicateInstruction": 1}`
//! * struct variant          -> `{"InsufficientFundsForRent": {"account_index": 2}}`
//! * `InstructionError(u8, InstructionError)` -> `{"InstructionError": [idx, <inner>]}`
//!   where `<inner>` is itself a unit string, `{"Custom": n}`, or (older nodes)
//!   `{"BorshIoError": "msg"}`.
//!
//! Anything that does not match is reported as `Unknown(<raw json>)`. Parsing
//! is total: it never panics and never recurses.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Every `TransactionError` variant name in the SDK source.
pub const TRANSACTION_ERROR_VARIANTS: &[&str] = &[
    "AccountInUse",
    "AccountLoadedTwice",
    "AccountNotFound",
    "ProgramAccountNotFound",
    "InsufficientFundsForFee",
    "InvalidAccountForFee",
    "AlreadyProcessed",
    "BlockhashNotFound",
    "InstructionError",
    "CallChainTooDeep",
    "MissingSignatureForFee",
    "InvalidAccountIndex",
    "SignatureFailure",
    "InvalidProgramForExecution",
    "SanitizeFailure",
    "ClusterMaintenance",
    "AccountBorrowOutstanding",
    "WouldExceedMaxBlockCostLimit",
    "UnsupportedVersion",
    "InvalidWritableAccount",
    "WouldExceedMaxAccountCostLimit",
    "WouldExceedAccountDataBlockLimit",
    "TooManyAccountLocks",
    "AddressLookupTableNotFound",
    "InvalidAddressLookupTableOwner",
    "InvalidAddressLookupTableData",
    "InvalidAddressLookupTableIndex",
    "InvalidRentPayingAccount",
    "WouldExceedMaxVoteCostLimit",
    "WouldExceedAccountDataTotalLimit",
    "DuplicateInstruction",
    "InsufficientFundsForRent",
    "MaxLoadedAccountsDataSizeExceeded",
    "InvalidLoadedAccountsDataSizeLimit",
    "ResanitizationNeeded",
    "ProgramExecutionTemporarilyRestricted",
    "UnbalancedTransaction",
    "ProgramCacheHitMaxLimit",
    "CommitCancelled",
    "BailOut",
];

/// Every `InstructionError` variant name in the SDK source.
pub const INSTRUCTION_ERROR_VARIANTS: &[&str] = &[
    "GenericError",
    "InvalidArgument",
    "InvalidInstructionData",
    "InvalidAccountData",
    "AccountDataTooSmall",
    "InsufficientFunds",
    "IncorrectProgramId",
    "MissingRequiredSignature",
    "AccountAlreadyInitialized",
    "UninitializedAccount",
    "UnbalancedInstruction",
    "ModifiedProgramId",
    "ExternalAccountLamportSpend",
    "ExternalAccountDataModified",
    "ReadonlyLamportChange",
    "ReadonlyDataModified",
    "DuplicateAccountIndex",
    "ExecutableModified",
    "RentEpochModified",
    "NotEnoughAccountKeys",
    "AccountDataSizeChanged",
    "AccountNotExecutable",
    "AccountBorrowFailed",
    "AccountBorrowOutstanding",
    "DuplicateAccountOutOfSync",
    "Custom",
    "InvalidError",
    "ExecutableDataModified",
    "ExecutableLamportChange",
    "ExecutableAccountNotRentExempt",
    "UnsupportedProgramId",
    "CallDepth",
    "MissingAccount",
    "ReentrancyNotAllowed",
    "MaxSeedLengthExceeded",
    "InvalidSeeds",
    "InvalidRealloc",
    "ComputationalBudgetExceeded",
    "PrivilegeEscalation",
    "ProgramEnvironmentSetupFailure",
    "ProgramFailedToComplete",
    "ProgramFailedToCompile",
    "Immutable",
    "IncorrectAuthority",
    "BorshIoError",
    "AccountNotRentExempt",
    "InvalidAccountOwner",
    "ArithmeticOverflow",
    "UnsupportedSysvar",
    "IllegalOwner",
    "MaxAccountsDataAllocationsExceeded",
    "MaxAccountsExceeded",
    "MaxInstructionTraceLengthExceeded",
    "BuiltinProgramsMustConsumeComputeUnits",
    "BailOut",
];

/// Cap on how much raw JSON is echoed back inside `Unknown(...)`.
const MAX_RAW_CHARS: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSource {
    Transaction,
    Instruction,
}

/// A normalised view of an RPC `err` value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedError {
    pub source: ErrorSource,
    /// Variant name, or `Unknown(<raw json>)` when it is not a known variant.
    pub kind: String,
    /// Failing instruction, from `InstructionError[i, _]` or `DuplicateInstruction(i)`.
    pub instruction_index: Option<u8>,
    /// Payload of `InstructionError[_, {"Custom": n}]`, when it fits in a u32.
    pub custom_code: Option<u32>,
    /// `account_index` of `InsufficientFundsForRent` / `ProgramExecutionTemporarilyRestricted`.
    pub account_index: Option<u8>,
}

impl ParsedError {
    pub fn is_unknown(&self) -> bool {
        self.kind.starts_with("Unknown(")
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn unknown(source: ErrorSource, raw: &Value, instruction_index: Option<u8>) -> ParsedError {
    let rendered = truncate_chars(&raw.to_string(), MAX_RAW_CHARS);
    ParsedError {
        source,
        kind: format!("Unknown({})", rendered),
        instruction_index,
        custom_code: None,
        account_index: None,
    }
}

fn known(source: ErrorSource, kind: &str) -> ParsedError {
    ParsedError {
        source,
        kind: kind.to_string(),
        instruction_index: None,
        custom_code: None,
        account_index: None,
    }
}

fn as_u8(value: &Value) -> Option<u8> {
    value.as_u64().and_then(|n| u8::try_from(n).ok())
}

/// Parses any JSON value into a [`ParsedError`]. Total and non-recursive.
pub fn parse_rpc_error(err: &Value) -> ParsedError {
    match err {
        Value::String(name) => parse_bare_name(name, err),
        Value::Object(map) if map.len() == 1 => match map.iter().next() {
            Some((key, payload)) if key == "InstructionError" => {
                parse_instruction_error(payload, err)
            }
            Some((key, payload)) => parse_transaction_variant(key, payload, err),
            None => unknown(ErrorSource::Transaction, err, None),
        },
        _ => unknown(ErrorSource::Transaction, err, None),
    }
}

fn parse_bare_name(name: &str, raw: &Value) -> ParsedError {
    if name != "InstructionError" && TRANSACTION_ERROR_VARIANTS.contains(&name) {
        return known(ErrorSource::Transaction, name);
    }
    // Some clients hand us just the inner InstructionError string; accept it
    // as an instruction-level error with no known index.
    if INSTRUCTION_ERROR_VARIANTS.contains(&name) {
        return known(ErrorSource::Instruction, name);
    }
    unknown(ErrorSource::Transaction, raw, None)
}

fn parse_transaction_variant(key: &str, payload: &Value, raw: &Value) -> ParsedError {
    if !TRANSACTION_ERROR_VARIANTS.contains(&key) {
        return unknown(ErrorSource::Transaction, raw, None);
    }
    let mut parsed = known(ErrorSource::Transaction, key);
    match key {
        // DuplicateInstruction(u8) carries the offending instruction index.
        "DuplicateInstruction" => parsed.instruction_index = as_u8(payload),
        "InsufficientFundsForRent" | "ProgramExecutionTemporarilyRestricted" => {
            parsed.account_index = payload.get("account_index").and_then(as_u8);
        }
        _ => {}
    }
    parsed
}

fn parse_instruction_error(payload: &Value, raw: &Value) -> ParsedError {
    let pair = match payload.as_array() {
        Some(arr) if arr.len() == 2 => arr,
        _ => return unknown(ErrorSource::Instruction, raw, None),
    };
    let idx = as_u8(&pair[0]);
    let inner = &pair[1];

    match inner {
        Value::String(name) if INSTRUCTION_ERROR_VARIANTS.contains(&name.as_str()) => {
            let mut parsed = known(ErrorSource::Instruction, name);
            parsed.instruction_index = idx;
            parsed
        }
        Value::Object(map) if map.len() == 1 => match map.iter().next() {
            Some((key, value)) if INSTRUCTION_ERROR_VARIANTS.contains(&key.as_str()) => {
                let mut parsed = known(ErrorSource::Instruction, key);
                parsed.instruction_index = idx;
                if key == "Custom" {
                    parsed.custom_code = value.as_u64().and_then(|c| u32::try_from(c).ok());
                }
                parsed
            }
            _ => unknown(ErrorSource::Instruction, inner, idx),
        },
        _ => unknown(ErrorSource::Instruction, inner, idx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_unit_and_struct_variants() {
        let p = parse_rpc_error(&json!("BlockhashNotFound"));
        assert_eq!(p.source, ErrorSource::Transaction);
        assert_eq!(p.kind, "BlockhashNotFound");

        let p = parse_rpc_error(&json!({"InsufficientFundsForRent": {"account_index": 2}}));
        assert_eq!(p.kind, "InsufficientFundsForRent");
        assert_eq!(p.account_index, Some(2));
    }

    #[test]
    fn test_instruction_shapes() {
        let p = parse_rpc_error(&json!({"InstructionError": [1, "InvalidAccountData"]}));
        assert_eq!(p.source, ErrorSource::Instruction);
        assert_eq!(p.kind, "InvalidAccountData");
        assert_eq!(p.instruction_index, Some(1));

        let p = parse_rpc_error(&json!({"InstructionError": [3, {"Custom": 6001}]}));
        assert_eq!(p.kind, "Custom");
        assert_eq!(p.custom_code, Some(6001));
    }

    #[test]
    fn test_unknown_is_reported_not_panicked() {
        let p = parse_rpc_error(&json!({"BrandNewVariant": {"x": 1}}));
        assert!(p.is_unknown());
        assert!(p.kind.contains("BrandNewVariant"));

        let long = "\u{e9}".repeat(1_000);
        let p = parse_rpc_error(&json!(long));
        assert!(p.is_unknown());
        assert!(p.kind.chars().count() < 200);
    }
}
