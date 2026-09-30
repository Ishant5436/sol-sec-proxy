//! Program attribution: which program produced a failure.
//!
//! Two independent sources are used and cross-checked:
//! 1. the failing instruction index mapped through the message's compiled
//!    instructions to `account_keys[program_id_index]` (program ids of invoked
//!    instructions are always static keys, never address lookup table entries);
//! 2. the last `Program <id> failed: <reason>` log line, which is the
//!    top-level program because inner CPI failures are logged first.

use crate::wire::ParsedTransaction;

/// Well-known program ids. Each id was checked against the SDK / SPL sources
/// or docs on 2026-09-30 (System, ComputeBudget: anza-xyz/solana-sdk sdk-ids;
/// Token, Token-2022, Associated Token Account, Memo v2: spl.solana.com).
pub const WELL_KNOWN_PROGRAMS: &[(&str, &str)] = &[
    ("11111111111111111111111111111111", "System Program"),
    (
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "SPL Token Program",
    ),
    (
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
        "SPL Token-2022 Program",
    ),
    (
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
        "Associated Token Account Program",
    ),
    (
        "ComputeBudget111111111111111111111111111111",
        "Compute Budget Program",
    ),
    (
        "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr",
        "SPL Memo Program",
    ),
];

/// Upper bound on log lines scanned; RPC nodes truncate logs far below this.
const MAX_LOG_LINES_SCANNED: usize = 10_000;

/// A `Program <id> failed: <reason>` log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFailure {
    pub program_id: String,
    pub reason: String,
}

pub fn program_name(program_id: &str) -> Option<&'static str> {
    WELL_KNOWN_PROGRAMS
        .iter()
        .find(|(id, _)| *id == program_id)
        .map(|(_, name)| *name)
}

/// Program id (base58) of the instruction at `instruction_index`, or `None`
/// if the index or the program key index is out of range.
pub fn program_id_for_instruction(tx: &ParsedTransaction, instruction_index: u8) -> Option<String> {
    let ix = tx.instructions.get(usize::from(instruction_index))?;
    let key = tx.account_keys.get(usize::from(ix.program_id_index))?;
    Some(bs58::encode(key).into_string())
}

/// Parses one log line of the form `Program <id> failed: <reason>`.
pub fn parse_failed_line(line: &str) -> Option<LogFailure> {
    let rest = line.strip_prefix("Program ")?;
    let (id, reason) = rest.split_once(" failed: ")?;
    let plausible_id =
        (32..=44).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric());
    if !plausible_id {
        return None;
    }
    Some(LogFailure {
        program_id: id.to_string(),
        reason: reason.trim().to_string(),
    })
}

/// The last `Program <id> failed:` line in the logs, scanning backwards.
pub fn last_failure_in_logs(logs: &[String]) -> Option<LogFailure> {
    logs.iter()
        .rev()
        .take(MAX_LOG_LINES_SCANNED)
        .find_map(|line| parse_failed_line(line))
}

/// Result of combining the two attribution sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    pub program_id: Option<String>,
    /// `Some(true)` if both sources exist and agree, `Some(false)` if they differ.
    pub sources_agree: Option<bool>,
}

/// Prefers the instruction-index mapping and falls back to the logs.
pub fn attribute(
    tx: Option<&ParsedTransaction>,
    instruction_index: Option<u8>,
    logs: &[String],
) -> Attribution {
    let from_ix = match (tx, instruction_index) {
        (Some(t), Some(i)) => program_id_for_instruction(t, i),
        _ => None,
    };
    let from_logs = last_failure_in_logs(logs).map(|f| f.program_id);

    let sources_agree = match (&from_ix, &from_logs) {
        (Some(a), Some(b)) => Some(a == b),
        _ => None,
    };
    Attribution {
        program_id: from_ix.or(from_logs),
        sources_agree,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_well_known_ids_decode_to_32_bytes() {
        for (id, _) in WELL_KNOWN_PROGRAMS {
            let bytes = bs58::decode(id).into_vec().unwrap_or_default();
            assert_eq!(bytes.len(), 32, "{}", id);
        }
    }

    #[test]
    fn test_parse_failed_line() {
        let f = parse_failed_line(
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA failed: custom program error: 0x1",
        );
        let f = f.unwrap_or_else(|| LogFailure {
            program_id: String::new(),
            reason: String::new(),
        });
        assert_eq!(program_name(&f.program_id), Some("SPL Token Program"));
        assert_eq!(f.reason, "custom program error: 0x1");
        assert!(parse_failed_line("Program log: Program X failed: no").is_none());
    }
}
