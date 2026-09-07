use std::hint::black_box;
use std::time::Instant;

use sol_sec_proxy::config::ProxyConfig;
use sol_sec_proxy::cu_optimizer::calculate_optimal_compute_units;
use sol_sec_proxy::error_decoder::{decode_anchor_error_code, decode_simulation_error};
use sol_sec_proxy::interceptor::{InterceptionOutcome, SimulationInterceptor};
use sol_sec_proxy::rpc_client::SimulationRpcResponse;
use sol_sec_proxy::wire::{parse_transaction_wire, COMPUTE_BUDGET_PROGRAM_ID_STR};

fn build_synthetic_v0_wire() -> Vec<u8> {
    let mut buf = Vec::with_capacity(512);
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
        .expect("Valid CB program id");
    buf.extend_from_slice(&cb_key);

    // Recent blockhash
    buf.extend_from_slice(&[0x33u8; 32]);

    // Instructions: 1 instruction (SetComputeUnitLimit = 800_000)
    buf.push(1u8);
    buf.push(1u8); // program_id_index
    buf.push(0u8); // accounts count
    let limit: u32 = 800_000;
    let mut data = vec![2u8];
    data.extend_from_slice(&limit.to_le_bytes());
    buf.push(data.len() as u8);
    buf.extend_from_slice(&data);

    // Address Lookup Tables: 1 table
    buf.push(1u8);
    buf.extend_from_slice(&[0x44u8; 32]);
    // Writable indexes: [1, 3]
    buf.push(2u8);
    buf.push(1u8);
    buf.push(3u8);
    // Readonly indexes: [2, 4, 5]
    buf.push(3u8);
    buf.push(2u8);
    buf.push(4u8);
    buf.push(5u8);

    assert!(!buf.is_empty(), "Buffer must not be empty");
    assert!(buf.len() <= 512, "Buffer within bounds");
    buf
}

fn bench_wire_deserialization(wire_bytes: &[u8], iterations: usize) -> (f64, f64) {
    assert!(!wire_bytes.is_empty(), "Wire bytes non-empty");
    assert!(iterations > 0, "Iterations positive");

    let start = Instant::now();
    let mut idx = 0;
    while idx < iterations {
        idx += 1;
        let parsed = parse_transaction_wire(black_box(wire_bytes)).expect("Must parse");
        black_box(parsed);
    }
    let elapsed = start.elapsed();
    let total_secs = elapsed.as_secs_f64();
    let avg_ns = (total_secs / (iterations as f64)) * 1_000_000_000.0;
    let ops_per_sec = (iterations as f64) / total_secs;
    (avg_ns, ops_per_sec)
}

fn bench_cu_optimization(iterations: usize) -> (f64, f64) {
    assert!(iterations > 0, "Iterations positive");

    let start = Instant::now();
    let mut idx = 0;
    while idx < iterations {
        idx += 1;
        let rec = calculate_optimal_compute_units(
            black_box(74_210),
            black_box(Some(200_000)),
            black_box(15),
            1_400_000,
            1_000,
        );
        black_box(rec);
    }
    let elapsed = start.elapsed();
    let total_secs = elapsed.as_secs_f64();
    let avg_ns = (total_secs / (iterations as f64)) * 1_000_000_000.0;
    let ops_per_sec = (iterations as f64) / total_secs;
    (avg_ns, ops_per_sec)
}

fn bench_anchor_error_decoding(iterations: usize) -> (f64, f64) {
    assert!(iterations > 0, "Iterations positive");

    let logs = vec![
        "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]".to_string(),
        "Program log: AnchorError thrown in programs/vault/src/lib.rs:88. Error Code: ConstraintTokenMint. Error Number: 2014. Error Message: A token mint constraint was violated.".to_string(),
        "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA failed: custom program error: 0x7de".to_string(),
    ];

    let start = Instant::now();
    let mut idx = 0;
    while idx < iterations {
        idx += 1;
        let (cat, name, msg) = decode_anchor_error_code(black_box(2014));
        black_box((cat, name, msg));
        let decoded =
            decode_simulation_error(black_box(Some(0)), black_box(2014), black_box(&logs));
        black_box(decoded);
    }
    let elapsed = start.elapsed();
    let total_secs = elapsed.as_secs_f64();
    let avg_ns = (total_secs / (iterations as f64)) * 1_000_000_000.0;
    let ops_per_sec = (iterations as f64) / total_secs;
    (avg_ns, ops_per_sec)
}

fn bench_end_to_end_interception(wire_bytes: &[u8], iterations: usize) -> (f64, f64) {
    assert!(!wire_bytes.is_empty(), "Wire bytes non-empty");
    assert!(iterations > 0, "Iterations positive");

    let interceptor = SimulationInterceptor::new(ProxyConfig::default());
    let parsed_tx = parse_transaction_wire(wire_bytes).expect("Valid tx");
    let sim_response = SimulationRpcResponse {
        err: None,
        units_consumed: Some(65_400),
        logs: Some(vec!["Program log: instruction executed".to_string()]),
    };

    let start = Instant::now();
    let mut idx = 0;
    while idx < iterations {
        idx += 1;
        let outcome =
            interceptor.evaluate_simulation(black_box(&parsed_tx), black_box(&sim_response));
        match outcome {
            InterceptionOutcome::Passed(rec) => {
                black_box(rec.recommendation.recommended_cu);
            }
            InterceptionOutcome::Reverted(rev) => {
                black_box(rev.avoided_wasted_fee_lamports as u32);
            }
        };
    }
    let elapsed = start.elapsed();
    let total_secs = elapsed.as_secs_f64();
    let avg_ns = (total_secs / (iterations as f64)) * 1_000_000_000.0;
    let ops_per_sec = (iterations as f64) / total_secs;
    (avg_ns, ops_per_sec)
}

fn main() {
    println!("=== sol-sec-proxy Empirical Performance Benchmark ===");
    println!("Platform: Apple Silicon ARM64 | Compiler: rustc with opt-level=3, LTO=true\n");

    let wire_bytes = build_synthetic_v0_wire();
    let iterations = 100_000;

    let (wire_ns, wire_ops) = bench_wire_deserialization(&wire_bytes, iterations);
    let (cu_ns, cu_ops) = bench_cu_optimization(iterations);
    let (err_ns, err_ops) = bench_anchor_error_decoding(iterations);
    let (e2e_ns, e2e_ops) = bench_end_to_end_interception(&wire_bytes, iterations);

    println!(
        "{:<40} | {:>12} | {:>16}",
        "Benchmark Suite (Phase)", "Latency (ns)", "Throughput (ops/s)"
    );
    println!("{:-<40}-+-{:-<12}-+-{:-<16}", "", "", "");
    println!(
        "{:<40} | {:>10.2} ns | {:>14.0} /s",
        "1. Wire Deserialization (v0 + ALT)", wire_ns, wire_ops
    );
    println!(
        "{:<40} | {:>10.2} ns | {:>14.0} /s",
        "2. Dynamic CU Optimization Engine", cu_ns, cu_ops
    );
    println!(
        "{:<40} | {:>10.2} ns | {:>14.0} /s",
        "3. Anchor Error & Log Translation", err_ns, err_ops
    );
    println!(
        "{:<40} | {:>10.2} ns | {:>14.0} /s",
        "4. End-to-End Interception Pipeline", e2e_ns, e2e_ops
    );

    println!(
        "\nTotal End-to-End Processing Overhead: {:.3} microseconds per transaction",
        e2e_ns / 1000.0
    );
    println!(
        "Empirical Throughput Capacity: {:.0} transactions/second per core",
        e2e_ops
    );
}
