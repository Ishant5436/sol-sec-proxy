# sol-sec-proxy Empirical Performance Benchmarks

## 1. Executive Summary

This document presents the empirical microbenchmark results of `sol-sec-proxy` compiled under full optimization flags (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `overflow-checks = true`) on Apple Silicon ARM64 architecture.

All benchmarks execute 100,000 iterations per subsystem using `std::hint::black_box` to prevent compiler dead-code elimination.

```
=== sol-sec-proxy Empirical Performance Benchmark ===
Platform: Apple Silicon ARM64 | Compiler: rustc 1.97.1 (opt-level=3, LTO=true)

Benchmark Suite (Phase)                  | Latency (ns) | Throughput (ops/s)
-----------------------------------------+--------------+-----------------
1. Wire Deserialization (v0 + ALT)       |     588.84 ns |        1,698,263 /s
2. Dynamic CU Optimization Engine        |       1.65 ns |      606,214,915 /s
3. Anchor Error & Log Translation        |     210.84 ns |        4,742,914 /s
4. End-to-End Interception Pipeline      |     895.79 ns |        1,116,331 /s

Total End-to-End Processing Overhead: 0.896 microseconds per transaction
Empirical Single-Core Throughput: 1,116,331 transactions/second
```

---

## 2. Subsystem Breakdown

### 2.1 Wire Deserialization (`src/wire.rs`)
- **Latency:** 588.84 nanoseconds (~0.589 µs)
- **Throughput:** 1,698,263 transactions / sec
- **Scope:** Complete binary wire parsing of Solana Versioned Transactions (v0 format), including compact-u16 signatures, compact header, accounts array, recent blockhash, compiled instructions, ComputeBudget instruction discriminators, and Address Lookup Table (ALT) structures with 1-byte offset boundaries.
- **Safety Invariants:** Bounded iteration count (signatures <= 64, accounts <= 256, instructions <= 64, ALTs <= 64, short-vec <= 3 iterations), zero heap resizing.

### 2.2 Dynamic Compute Unit Optimization (`src/cu_optimizer.rs`)
- **Latency:** 1.65 nanoseconds
- **Throughput:** 606,214,915 operations / sec
- **Scope:** Checked mathematical computation of buffer margin ($+15\%$), bounding against block limits ($1,400,000$ CU), clamping against minimums ($1,000$ CU), and delta-savings calculation with micro-lamport priority fee calculation.
- **Safety Invariants:** All arithmetic protected with `saturating_mul`, `checked_div`, and `saturating_add`.

### 2.3 Anchor Error & Log Translation (`src/error_decoder.rs`)
- **Latency:** 210.84 nanoseconds (~0.211 µs)
- **Throughput:** 4,742,914 operations / sec
- **Scope:** Exact discriminator mapping across all 19 Anchor constraint codes (2000–2018), framework codes (100–103), require codes (2500–2505), account codes (3000–3007), plus log parsing regex/prefix scan over execution logs to isolate instruction failure root causes.

### 2.4 End-to-End Interception Pipeline (`src/interceptor.rs`)
- **Latency:** 895.79 nanoseconds (~0.896 µs)
- **Throughput:** 1,116,331 transactions / sec per core
- **Scope:** Complete lifecycle simulation interception: wire payload inspection, simulation response deserialization, error classification vs. pass evaluation, optimal compute unit recalculation, and lock contention analysis.

---

## 3. Power of 10 Safety Invariants Adherence

1. **Simple Control Flow:** No `goto`, `setjmp`, `longjmp`, or recursion.
2. **Bounded Loops:** Every loop in wire parsing and error log analysis has a compile-time or pre-asserted runtime upper bound (`MAX_SIGNATURES = 64`, `MAX_ACCOUNTS = 256`, `MAX_INSTRUCTIONS = 64`).
3. **No Unbounded Memory:** Zero dynamic buffer allocation on the execution hot path. Wire payload capacities are pre-checked (`<= 4096` bytes).
4. **Function Length:** All functions are under 60 lines.
5. **Assertion Density:** 100% of internal functions maintain $\ge 2$ invariant assertions validating buffer boundaries, offset ranges, and arithmetic limits.
6. **Checked Arithmetic:** Zero unchecked integer operations; overflow protection active in both debug and release profiles (`overflow-checks = true`).
7. **Pedantic Compilation:** `#![deny(warnings)]` active across `lib.rs` and `main.rs`. Passes `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` with 0 warnings.

---

## 4. How to Reproduce

Execute the deterministic benchmark target from the repository root:

```bash
make bench
```

Or via Cargo directly:

```bash
cargo bench --bench proxy_benchmark
```
