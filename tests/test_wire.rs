use sol_sec_proxy::wire::{
    parse_transaction_wire, TransactionVersion, COMPUTE_BUDGET_PROGRAM_ID_STR,
};

#[test]
fn test_parse_wire_empty_fails() {
    let empty = [];
    assert!(std::panic::catch_unwind(|| parse_transaction_wire(&empty)).is_err());
}

#[test]
fn test_parse_wire_legacy_synthetic() {
    let mut buf = Vec::new();
    // 1 signature
    buf.push(1u8);
    buf.extend_from_slice(&[7u8; 64]);

    // Legacy message header (first byte high bit is 0)
    buf.push(1u8); // num_required_signatures
    buf.push(0u8); // num_readonly_signed_accounts
    buf.push(1u8); // num_readonly_unsigned_accounts

    // Accounts: 2 accounts
    buf.push(2u8);
    buf.extend_from_slice(&[1u8; 32]); // Fee payer / signer
    let cb_key = bs58::decode(COMPUTE_BUDGET_PROGRAM_ID_STR)
        .into_vec()
        .unwrap();
    buf.extend_from_slice(&cb_key); // ComputeBudget program

    // Recent blockhash
    buf.extend_from_slice(&[0xaau8; 32]);

    // Instructions: 1 instruction (SetComputeUnitLimit = 400_000)
    buf.push(1u8); // 1 instruction
    buf.push(1u8); // program_id_index = 1 (ComputeBudget)
    buf.push(0u8); // 0 accounts
                   // instruction data: [2, 400_000 in LE u32]
    let limit: u32 = 400_000;
    let mut data = vec![2u8];
    data.extend_from_slice(&limit.to_le_bytes());
    buf.push(data.len() as u8);
    buf.extend_from_slice(&data);

    let parsed = parse_transaction_wire(&buf).expect("Should parse synthetic transaction");
    assert_eq!(parsed.version, TransactionVersion::Legacy);
    assert_eq!(parsed.signatures.len(), 1);
    assert_eq!(parsed.signatures[0], [7u8; 64]);
    assert_eq!(parsed.account_keys.len(), 2);
    assert_eq!(parsed.recent_blockhash, [0xaau8; 32]);
    assert_eq!(parsed.instructions.len(), 1);
    assert_eq!(parsed.compute_unit_limit, Some(400_000));
    assert_eq!(parsed.compute_unit_price, None);
}

#[test]
fn test_parse_wire_v0_synthetic() {
    let mut buf = Vec::new();
    // 1 signature
    buf.push(1u8);
    buf.extend_from_slice(&[9u8; 64]);

    // V0 message indicator: 0x80 | 0 = 0x80
    buf.push(0x80u8);
    // Header
    buf.push(1u8); // num_required_signatures
    buf.push(0u8); // num_readonly_signed
    buf.push(1u8); // num_readonly_unsigned

    // Accounts: 2 accounts
    buf.push(2u8);
    buf.extend_from_slice(&[2u8; 32]);
    let cb_key = bs58::decode(COMPUTE_BUDGET_PROGRAM_ID_STR)
        .into_vec()
        .unwrap();
    buf.extend_from_slice(&cb_key);

    // Recent blockhash
    buf.extend_from_slice(&[0xbbu8; 32]);

    // Instructions: 1 instruction (SetComputeUnitPrice = 50_000 micro-lamports)
    buf.push(1u8);
    buf.push(1u8); // program_id_index
    buf.push(0u8); // accounts count
    let price: u64 = 50_000;
    let mut data = vec![3u8];
    data.extend_from_slice(&price.to_le_bytes());
    buf.push(data.len() as u8);
    buf.extend_from_slice(&data);

    let parsed = parse_transaction_wire(&buf).expect("Should parse v0 transaction");
    assert_eq!(parsed.version, TransactionVersion::V0);
    assert_eq!(parsed.compute_unit_price, Some(50_000));
    assert!(parsed.address_table_lookups.is_empty());
}

#[test]
fn test_parse_wire_v0_with_address_lookup_tables() {
    let mut buf = Vec::new();
    // 1 signature
    buf.push(1u8);
    buf.extend_from_slice(&[0x11u8; 64]);

    // V0 indicator
    buf.push(0x80u8);
    // Header
    buf.push(1u8); // num_required_signatures
    buf.push(0u8); // num_readonly_signed
    buf.push(1u8); // num_readonly_unsigned

    // Accounts: 2 accounts
    buf.push(2u8);
    buf.extend_from_slice(&[0x22u8; 32]);
    let cb_key = bs58::decode(COMPUTE_BUDGET_PROGRAM_ID_STR)
        .into_vec()
        .unwrap();
    buf.extend_from_slice(&cb_key);

    // Recent blockhash
    buf.extend_from_slice(&[0x33u8; 32]);

    // Instructions: 1 instruction
    buf.push(1u8);
    buf.push(1u8); // program_id_index
    buf.push(0u8); // accounts count
    let limit: u32 = 800_000;
    let mut data = vec![2u8];
    data.extend_from_slice(&limit.to_le_bytes());
    buf.push(data.len() as u8);
    buf.extend_from_slice(&data);

    // Address Lookup Tables (ALTs): 1 table
    buf.push(1u8); // 1 ALT lookup
    buf.extend_from_slice(&[0x44u8; 32]); // ALT account key
                                          // Writable indexes: [1, 3]
    buf.push(2u8);
    buf.push(1u8);
    buf.push(3u8);
    // Readonly indexes: [2, 4, 5]
    buf.push(3u8);
    buf.push(2u8);
    buf.push(4u8);
    buf.push(5u8);

    let parsed = parse_transaction_wire(&buf).expect("Should parse v0 transaction with ALT");
    assert_eq!(parsed.version, TransactionVersion::V0);
    assert_eq!(parsed.compute_unit_limit, Some(800_000));
    assert_eq!(parsed.address_table_lookups.len(), 1);
    let alt = &parsed.address_table_lookups[0];
    assert_eq!(alt.account_key, [0x44u8; 32]);
    assert_eq!(alt.writable_indexes, vec![1, 3]);
    assert_eq!(alt.readonly_indexes, vec![2, 4, 5]);
}
