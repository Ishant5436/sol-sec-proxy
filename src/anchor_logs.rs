//! Extracts custom program error details from transaction logs, offline.
//!
//! Recognised forms:
//! 1. `Program log: AnchorError thrown in <file>:<line>. Error Code: <Name>. Error Number: <N>. Error Message: <msg>.`
//! 2. `Program log: AnchorError caused by account: <acct>. Error Code: <Name>. Error Number: <N>. Error Message: <msg>.`
//! 3. `Program <id> failed: custom program error: 0x<hex>`
//!
//! No IDLs are fetched; only what the logs themselves say is reported.

use crate::programs::last_failure_in_logs;
use serde::{Deserialize, Serialize};

/// Upper bound on log lines inspected for Anchor errors.
const MAX_LOG_LINES_SCANNED: usize = 10_000;

const MARK_CODE: &str = "Error Code: ";
const MARK_NUMBER: &str = "Error Number: ";
const MARK_MESSAGE: &str = "Error Message: ";
const MARK_ACCOUNT: &str = "caused by account: ";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomErrorInfo {
    pub code: Option<u32>,
    pub name: Option<String>,
    pub message: Option<String>,
    pub account: Option<String>,
}

impl CustomErrorInfo {
    pub fn is_empty(&self) -> bool {
        self.code.is_none()
            && self.name.is_none()
            && self.message.is_none()
            && self.account.is_none()
    }
}

/// Text between `start` and the next `end` marker (or the end of the line).
fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let rest = &text[from..];
    let to = rest.find(end).unwrap_or(rest.len());
    Some(rest[..to].trim())
}

/// Parses forms 1 and 2. Returns `None` for lines that are not Anchor errors.
pub fn parse_anchor_line(line: &str) -> Option<CustomErrorInfo> {
    let start = line.find("AnchorError")?;
    let body = &line[start..];
    if !body.contains(MARK_CODE) {
        return None;
    }

    let account = if body.contains(MARK_ACCOUNT) {
        between(body, MARK_ACCOUNT, ". Error Code:").map(str::to_string)
    } else {
        None
    };
    let name = between(body, MARK_CODE, ". Error Number:").map(str::to_string);
    let code = between(body, MARK_NUMBER, ". Error Message:").and_then(|n| n.parse::<u32>().ok());
    let message = body
        .find(MARK_MESSAGE)
        .map(|pos| body[pos + MARK_MESSAGE.len()..].trim().to_string())
        .filter(|m| !m.is_empty());

    Some(CustomErrorInfo {
        code,
        name: name.filter(|n| !n.is_empty()),
        message,
        account: account.filter(|a| !a.is_empty()),
    })
}

/// Parses `custom program error: 0x<hex>` (form 3) into its code.
pub fn parse_custom_error_reason(reason: &str) -> Option<u32> {
    let hex = reason.trim().strip_prefix("custom program error: 0x")?;
    u32::from_str_radix(hex.trim(), 16).ok()
}

/// Builds the best-available [`CustomErrorInfo`] from logs.
///
/// If `expected_code` is given (from the RPC `Custom(n)` error), the Anchor
/// line whose `Error Number` equals it is preferred, because a CPI chain can
/// log several Anchor errors. Otherwise the last Anchor line wins. The plain
/// `custom program error: 0x..` line only fills in a missing code.
pub fn extract_custom_error(logs: &[String], expected_code: Option<u32>) -> CustomErrorInfo {
    let mut best: Option<CustomErrorInfo> = None;
    for line in logs.iter().take(MAX_LOG_LINES_SCANNED) {
        if let Some(info) = parse_anchor_line(line) {
            let matches_expected = expected_code.is_some() && info.code == expected_code;
            let have_exact_match = best
                .as_ref()
                .map(|b| expected_code.is_some() && b.code == expected_code)
                .unwrap_or(false);
            if matches_expected || !have_exact_match {
                best = Some(info);
            }
        }
    }

    let mut info = best.unwrap_or_default();
    if info.code.is_none() {
        info.code = expected_code.or_else(|| {
            last_failure_in_logs(logs).and_then(|f| parse_custom_error_reason(&f.reason))
        });
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thrown_in_form() {
        let line = "Program log: AnchorError thrown in programs/dex/src/lib.rs:88. Error Code: SlippageExceeded. Error Number: 6001. Error Message: Price slippage limit exceeded.";
        let info = parse_anchor_line(line).unwrap_or_default();
        assert_eq!(info.name.as_deref(), Some("SlippageExceeded"));
        assert_eq!(info.code, Some(6001));
        assert_eq!(
            info.message.as_deref(),
            Some("Price slippage limit exceeded.")
        );
        assert_eq!(info.account, None);
    }

    #[test]
    fn test_caused_by_account_form() {
        let line = "Program log: AnchorError caused by account: vault. Error Code: ConstraintMut. Error Number: 2000. Error Message: A mut constraint was violated.";
        let info = parse_anchor_line(line).unwrap_or_default();
        assert_eq!(info.account.as_deref(), Some("vault"));
        assert_eq!(info.code, Some(2000));
    }

    #[test]
    fn test_hex_form_and_overflow() {
        assert_eq!(
            parse_custom_error_reason("custom program error: 0x1771"),
            Some(6001)
        );
        assert_eq!(
            parse_custom_error_reason("custom program error: 0xFFFFFFFFF"),
            None
        );
        assert_eq!(parse_custom_error_reason("custom program error: 0x"), None);
    }
}
