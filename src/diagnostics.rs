//! Agent-actionable failure diagnostics: one structured object that says what
//! failed, where, in which program, and what a caller should do next.

use crate::anchor_logs::{extract_custom_error, CustomErrorInfo};
use crate::programs::{attribute, program_name};
use crate::retry::{classify, RetryClass};
use crate::tx_error::{parse_rpc_error, ErrorSource};
use crate::wire::ParsedTransaction;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Lines kept in `logs_snippet`.
const MAX_SNIPPET_LINES: usize = 5;
/// Characters kept per snippet line (logs can carry very long base64 blobs).
const MAX_SNIPPET_LINE_CHARS: usize = 300;
/// Log lines inspected when building the snippet.
const MAX_LOG_LINES_SCANNED: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnosis {
    pub source: ErrorSource,
    pub kind: String,
    pub instruction_index: Option<u8>,
    pub program_id: Option<String>,
    pub program_name: Option<String>,
    pub custom_error: CustomErrorInfo,
    pub retry_class: RetryClass,
    pub suggested_action: String,
    pub logs_snippet: Vec<String>,
}

fn is_interesting(line: &str) -> bool {
    line.contains("failed") || line.contains("Error") || line.contains("panicked")
}

/// Last few failure-related lines. Failures are logged at the end, so scan
/// from the back and restore chronological order.
fn build_logs_snippet(logs: &[String]) -> Vec<String> {
    let mut picked: Vec<String> = logs
        .iter()
        .rev()
        .take(MAX_LOG_LINES_SCANNED)
        .filter(|line| is_interesting(line))
        .take(MAX_SNIPPET_LINES)
        .map(|line| line.chars().take(MAX_SNIPPET_LINE_CHARS).collect())
        .collect();
    picked.reverse();
    picked
}

fn compose_action(
    base: &str,
    custom: &CustomErrorInfo,
    account_index: Option<u8>,
    sources_agree: Option<bool>,
) -> String {
    let mut action = base.to_string();
    if let Some(name) = &custom.name {
        match custom.code {
            Some(code) => action.push_str(&format!(" Program error: {} ({}).", name, code)),
            None => action.push_str(&format!(" Program error: {}.", name)),
        }
    } else if let Some(code) = custom.code {
        action.push_str(&format!(" Program error code: {}.", code));
    }
    if let Some(idx) = account_index {
        action.push_str(&format!(" Affected account index: {}.", idx));
    }
    if sources_agree == Some(false) {
        action.push_str(
            " Note: the failing program in the logs differs from the instruction's program id.",
        );
    }
    action
}

/// Builds a [`Diagnosis`] from any RPC `err` value plus logs. Never panics.
/// `tx` is optional; without it, program attribution falls back to the logs.
pub fn diagnose(err: &Value, logs: &[String], tx: Option<&ParsedTransaction>) -> Diagnosis {
    let parsed = parse_rpc_error(err);
    let policy = classify(parsed.source, &parsed.kind);

    let custom = if parsed.source == ErrorSource::Instruction && parsed.kind == "Custom" {
        extract_custom_error(logs, parsed.custom_code)
    } else {
        CustomErrorInfo::default()
    };

    let attribution = attribute(tx, parsed.instruction_index, logs);
    let name = attribution
        .program_id
        .as_deref()
        .and_then(program_name)
        .map(str::to_string);

    let suggested_action = compose_action(
        policy.action,
        &custom,
        parsed.account_index,
        attribution.sources_agree,
    );

    // Internal invariants over values this module constructed, not over input.
    assert!(
        !parsed.kind.is_empty(),
        "kind is always a name or Unknown(..)"
    );
    assert!(
        !suggested_action.is_empty(),
        "action table rows are non-empty"
    );

    Diagnosis {
        source: parsed.source,
        kind: parsed.kind,
        instruction_index: parsed.instruction_index,
        program_id: attribution.program_id,
        program_name: name,
        custom_error: custom,
        retry_class: policy.class,
        suggested_action,
        logs_snippet: build_logs_snippet(logs),
    }
}
