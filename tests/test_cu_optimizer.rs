use sol_sec_proxy::cu_optimizer::{analyze_account_write_locks, calculate_optimal_compute_units};

#[test]
fn test_calculate_optimal_compute_units_various_loads() {
    // Zero units consumed clamps to 5,000 minimum
    let rec0 = calculate_optimal_compute_units(0, None, 15, 1_400_000, 100);
    assert_eq!(rec0.recommended_cu, 5_000);

    // Standard transfer: 3,000 units + 15% = 3,450 -> clamps to 5,000
    let rec_transfer = calculate_optimal_compute_units(3_000, Some(200_000), 15, 1_400_000, 1_000);
    assert_eq!(rec_transfer.recommended_cu, 5_000);
    assert_eq!(rec_transfer.potential_cu_saved, 195_000);
    assert_eq!(rec_transfer.estimated_priority_fee_saved_lamports, 195);

    // Heavy DeFi swap: 240,000 units + 15% = 276,000
    let rec_swap = calculate_optimal_compute_units(240_000, Some(400_000), 15, 1_400_000, 25_000);
    assert_eq!(rec_swap.recommended_cu, 276_000);
    assert_eq!(rec_swap.potential_cu_saved, 124_000);
    assert_eq!(rec_swap.estimated_priority_fee_saved_lamports, 3_100);
}

#[test]
fn test_analyze_account_write_locks() {
    let mut keys = Vec::new();
    for i in 0..6 {
        keys.push([i as u8; 32]);
    }
    // 6 accounts:
    // 2 signed, 1 ro signed => 1 rw signed (key 0)
    // 4 unsigned, 2 ro unsigned => 2 rw unsigned (key 2, 3)
    let analysis = analyze_account_write_locks(&keys, 2, 1, 2);
    assert_eq!(analysis.writable_accounts_count, 3);
    assert_eq!(analysis.readonly_accounts_count, 3);
    assert_eq!(analysis.writable_keys.len(), 3);
    assert!(!analysis.has_hot_lock_risk);
}
