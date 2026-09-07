use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuOptimizationRecommendation {
    pub units_consumed: u64,
    pub original_requested_cu: Option<u32>,
    pub recommended_cu: u32,
    pub safety_buffer_percent: u32,
    pub potential_cu_saved: u32,
    pub estimated_priority_fee_saved_lamports: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountLockAnalysis {
    pub writable_accounts_count: usize,
    pub readonly_accounts_count: usize,
    pub writable_keys: Vec<String>,
    pub has_hot_lock_risk: bool,
}

pub fn calculate_optimal_compute_units(
    units_consumed: u64,
    original_requested_cu: Option<u32>,
    buffer_percent: u32,
    max_cu_limit: u32,
    priority_fee_micro_lamports: u64,
) -> CuOptimizationRecommendation {
    assert!(
        units_consumed <= 1_400_000,
        "Units consumed bounded by max block limit"
    );
    assert!(buffer_percent <= 100, "Buffer percent bound");

    let buffer = (units_consumed * (buffer_percent as u64)) / 100;
    let target = units_consumed.saturating_add(buffer);

    let recommended = (target as u32).clamp(5_000, max_cu_limit);

    let (saved_cu, saved_fee) = match original_requested_cu {
        Some(original) if original > recommended => {
            let diff = original.saturating_sub(recommended);
            let fee_saved = ((diff as u64) * priority_fee_micro_lamports) / 1_000_000;
            (diff, fee_saved)
        }
        _ => (0, 0),
    };

    assert!(recommended >= 5_000, "Minimum CU bound");
    assert!(recommended <= max_cu_limit, "Maximum CU bound");

    CuOptimizationRecommendation {
        units_consumed,
        original_requested_cu,
        recommended_cu: recommended,
        safety_buffer_percent: buffer_percent,
        potential_cu_saved: saved_cu,
        estimated_priority_fee_saved_lamports: saved_fee,
    }
}

pub fn analyze_account_write_locks(
    account_keys: &[[u8; 32]],
    num_required_signatures: u8,
    num_readonly_signed: u8,
    num_readonly_unsigned: u8,
) -> AccountLockAnalysis {
    assert!(
        account_keys.len() >= num_required_signatures as usize,
        "Signature keys invariant"
    );
    assert!(account_keys.len() <= 256, "Max accounts upper bound");

    let total = account_keys.len();
    let num_signed = num_required_signatures as usize;
    let num_ro_signed = num_readonly_signed as usize;
    let num_ro_unsigned = num_readonly_unsigned as usize;

    let num_rw_signed = num_signed.saturating_sub(num_ro_signed);
    let num_unsigned = total.saturating_sub(num_signed);
    let num_rw_unsigned = num_unsigned.saturating_sub(num_ro_unsigned);

    let total_writable = num_rw_signed + num_rw_unsigned;
    let total_readonly = total.saturating_sub(total_writable);

    let mut writable_keys = Vec::with_capacity(total_writable);

    let mut i = 0;
    while i < num_rw_signed && i < total {
        writable_keys.push(bs58::encode(&account_keys[i]).into_string());
        i += 1;
    }

    let mut j = num_signed;
    let end_unsigned_rw = num_signed + num_rw_unsigned;
    while j < end_unsigned_rw && j < total {
        writable_keys.push(bs58::encode(&account_keys[j]).into_string());
        j += 1;
    }

    let has_hot_lock_risk = total_writable >= 8;

    assert_eq!(
        writable_keys.len(),
        total_writable,
        "Writable count matches vector length"
    );
    assert!(
        total_writable <= total,
        "Writable accounts cannot exceed total"
    );

    AccountLockAnalysis {
        writable_accounts_count: total_writable,
        readonly_accounts_count: total_readonly,
        writable_keys,
        has_hot_lock_risk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cu_optimization_overpay() {
        let rec = calculate_optimal_compute_units(
            100_000,
            Some(1_000_000),
            15,
            1_400_000,
            50_000, // 50,000 micro-lamports per CU
        );
        assert_eq!(rec.units_consumed, 100_000);
        assert_eq!(rec.recommended_cu, 115_000);
        assert_eq!(rec.potential_cu_saved, 885_000);
        // fee saved = (885_000 * 50_000) / 1_000_000 = 44,250 lamports
        assert_eq!(rec.estimated_priority_fee_saved_lamports, 44_250);
    }

    #[test]
    fn test_cu_clamp_boundaries() {
        let rec_min = calculate_optimal_compute_units(1_000, None, 15, 1_400_000, 0);
        assert_eq!(rec_min.recommended_cu, 5_000);

        let rec_max = calculate_optimal_compute_units(1_350_000, None, 20, 1_400_000, 0);
        assert_eq!(rec_max.recommended_cu, 1_400_000);
    }
}
