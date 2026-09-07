use sol_sec_proxy::error_decoder::{decode_anchor_error_code, decode_simulation_error};

#[test]
fn test_all_anchor_framework_ranges() {
    let framework_codes = [100, 101, 102, 103];
    for code in framework_codes {
        let (cat, name, msg) = decode_anchor_error_code(code);
        assert_eq!(cat, "AnchorFramework");
        assert!(!name.is_empty());
        assert!(!msg.is_empty());
    }

    let constraint_codes = [
        2000, 2001, 2002, 2003, 2004, 2005, 2006, 2007, 2008, 2009, 2012, 2014, 2015,
    ];
    for code in constraint_codes {
        let (cat, name, msg) = decode_anchor_error_code(code);
        assert_eq!(cat, "AnchorConstraint");
        assert!(!name.is_empty());
        assert!(!msg.is_empty());
    }

    let require_codes = [2500, 2501, 2502, 2503, 2504, 2505];
    for code in require_codes {
        let (cat, name, msg) = decode_anchor_error_code(code);
        assert_eq!(cat, "AnchorRequire");
        assert!(!name.is_empty());
        assert!(!msg.is_empty());
    }

    let account_codes = [3000, 3001, 3002, 3003, 3004, 3005, 3006, 3007];
    for code in account_codes {
        let (cat, name, msg) = decode_anchor_error_code(code);
        assert_eq!(cat, "AnchorAccount");
        assert!(!name.is_empty());
        assert!(!msg.is_empty());
    }
}

#[test]
fn test_custom_error_log_fallback() {
    let logs = vec![
        "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]".to_string(),
        "Program log: Error: insufficient funds".to_string(),
        "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA failed: custom program error: 0x1"
            .to_string(),
    ];

    let decoded = decode_simulation_error(Some(1), 1, &logs);
    assert_eq!(decoded.category, "SolanaCustom");
    assert_eq!(decoded.error_code, 1);
    assert_eq!(decoded.logs_snippet.len(), 2);
}
