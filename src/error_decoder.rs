use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedError {
    pub instruction_index: Option<u8>,
    pub error_code: u32,
    pub category: String,
    pub error_name: String,
    pub message: String,
    pub logs_snippet: Vec<String>,
}

pub fn decode_anchor_error_code(code: u32) -> (&'static str, &'static str, &'static str) {
    assert!(code <= 1_000_000, "Error code upper bound invariant");
    match code {
        // Framework instruction errors (100-199)
        100 => (
            "AnchorFramework",
            "InstructionMissing",
            "8-byte instruction discriminator not provided",
        ),
        101 => (
            "AnchorFramework",
            "InstructionFallbackNotFound",
            "Fallback instruction handler not found",
        ),
        102 => (
            "AnchorFramework",
            "InstructionDidNotDeserialize",
            "Instruction data deserialization failed",
        ),
        103 => (
            "AnchorFramework",
            "InstructionDidNotSerialize",
            "Instruction data serialization failed",
        ),

        // Constraint violations (2000-2099)
        2000 => (
            "AnchorConstraint",
            "ConstraintMut",
            "A mut constraint was violated (account not writable)",
        ),
        2001 => (
            "AnchorConstraint",
            "ConstraintHasOne",
            "A has_one constraint was violated",
        ),
        2002 => (
            "AnchorConstraint",
            "ConstraintSigner",
            "A signer constraint was violated (account did not sign)",
        ),
        2003 => (
            "AnchorConstraint",
            "ConstraintRaw",
            "A raw constraint expression evaluated to false",
        ),
        2004 => (
            "AnchorConstraint",
            "ConstraintOwner",
            "An owner constraint was violated",
        ),
        2005 => (
            "AnchorConstraint",
            "ConstraintRentExempt",
            "A rent exemption constraint was violated",
        ),
        2006 => (
            "AnchorConstraint",
            "ConstraintSeeds",
            "A seeds PDA constraint was violated",
        ),
        2007 => (
            "AnchorConstraint",
            "ConstraintExecutable",
            "An executable constraint was violated",
        ),
        2008 => (
            "AnchorConstraint",
            "ConstraintStateKeyMismatch",
            "State account key mismatch",
        ),
        2009 => (
            "AnchorConstraint",
            "ConstraintAssociated",
            "Associated token constraint violated",
        ),
        2012 => (
            "AnchorConstraint",
            "ConstraintAddress",
            "An address constraint was violated",
        ),
        2014 => (
            "AnchorConstraint",
            "ConstraintTokenMint",
            "Token mint constraint violated",
        ),
        2015 => (
            "AnchorConstraint",
            "ConstraintTokenOwner",
            "Token account owner constraint violated",
        ),

        // Require expressions (2500-2599)
        2500 => (
            "AnchorRequire",
            "RequireEqViolated",
            "require_eq comparison expression failed",
        ),
        2501 => (
            "AnchorRequire",
            "RequireNeqViolated",
            "require_neq comparison expression failed",
        ),
        2502 => (
            "AnchorRequire",
            "RequireGtViolated",
            "require_gt comparison expression failed",
        ),
        2503 => (
            "AnchorRequire",
            "RequireGteViolated",
            "require_gte comparison expression failed",
        ),
        2504 => (
            "AnchorRequire",
            "RequireLtViolated",
            "require_lt comparison expression failed",
        ),
        2505 => (
            "AnchorRequire",
            "RequireLteViolated",
            "require_lte comparison expression failed",
        ),

        // Account lifecycle errors (3000-3099)
        3000 => (
            "AnchorAccount",
            "AccountDiscriminatorAlreadySet",
            "Discriminator already initialized on account",
        ),
        3001 => (
            "AnchorAccount",
            "AccountDiscriminatorNotFound",
            "Account discriminator not initialized",
        ),
        3002 => (
            "AnchorAccount",
            "AccountDiscriminatorMismatch",
            "Account 8-byte discriminator mismatch",
        ),
        3003 => (
            "AnchorAccount",
            "AccountDidNotDeserialize",
            "Account data failed Borsh deserialization",
        ),
        3004 => (
            "AnchorAccount",
            "AccountDidNotSerialize",
            "Account data failed serialization",
        ),
        3005 => (
            "AnchorAccount",
            "AccountNotEnoughKeys",
            "Fewer account keys provided than declared",
        ),
        3006 => (
            "AnchorAccount",
            "AccountNotMutable",
            "Target account marked readonly but needs mutation",
        ),
        3007 => (
            "AnchorAccount",
            "AccountOwnedByWrongProgram",
            "Account owner is not the current program",
        ),

        // Custom program-defined errors (6000+)
        code if code >= 6000 => (
            "AnchorCustom",
            "CustomProgramError",
            "Custom program error code thrown by Anchor contract",
        ),

        // Unrecognized or native code
        _ => (
            "SolanaCustom",
            "UnknownCustomError",
            "Unknown custom program error code",
        ),
    }
}

pub fn decode_simulation_error(
    instruction_idx: Option<u8>,
    custom_code: u32,
    logs: &[String],
) -> DecodedError {
    assert!(custom_code <= 1_000_000, "Custom code validation bound");
    assert!(logs.len() <= 512, "Log count upper bound");

    let (category, name, fallback_msg) = decode_anchor_error_code(custom_code);

    let mut message = fallback_msg.to_string();
    let mut logs_snippet = Vec::new();

    let mut loop_idx = 0;
    while loop_idx < logs.len() && loop_idx < 100 {
        let line = &logs[loop_idx];
        loop_idx += 1;

        if line.contains("Error Message:") {
            if let Some(pos) = line.find("Error Message:") {
                let extracted = line[pos + "Error Message:".len()..].trim();
                if !extracted.is_empty() {
                    message = extracted.to_string();
                }
            }
        }
        if (line.contains("failed") || line.contains("Error") || line.contains("panicked"))
            && logs_snippet.len() < 5
        {
            logs_snippet.push(line.clone());
        }
    }

    assert!(!category.is_empty(), "Category invariant");
    assert!(!name.is_empty(), "Name invariant");

    DecodedError {
        instruction_index: instruction_idx,
        error_code: custom_code,
        category: category.to_string(),
        error_name: name.to_string(),
        message,
        logs_snippet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_code_decoding() {
        let (cat, name, msg) = decode_anchor_error_code(2003);
        assert_eq!(cat, "AnchorConstraint");
        assert_eq!(name, "ConstraintRaw");
        assert!(msg.contains("raw constraint"));

        let (c_cat, c_name, _) = decode_anchor_error_code(6005);
        assert_eq!(c_cat, "AnchorCustom");
        assert_eq!(c_name, "CustomProgramError");
    }

    #[test]
    fn test_simulation_error_log_extraction() {
        let logs = vec![
            "Program 11111111111111111111111111111111 invoke [1]".to_string(),
            "Program log: AnchorError thrown in programs/my_dex/src/lib.rs:88. Error Code: SlippageExceeded. Error Number: 6001. Error Message: Price slippage limit exceeded.".to_string(),
            "Program 11111111111111111111111111111111 failed: custom program error: 0x1771".to_string(),
        ];

        let decoded = decode_simulation_error(Some(2), 6001, &logs);
        assert_eq!(decoded.instruction_index, Some(2));
        assert_eq!(decoded.error_code, 6001);
        assert_eq!(decoded.message, "Price slippage limit exceeded.");
        assert_eq!(decoded.logs_snippet.len(), 2);
    }
}
