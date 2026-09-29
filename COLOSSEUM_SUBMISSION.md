# Colosseum Crypto World's Fair 2026: Project Submission Dossier

**Project Name:** sol-sec-proxy  
**Tagline:** Deterministic Pre-Execution Revert Firewall & Dynamic Compute-Budget Optimizer for Solana  
**Track:** Solana Ecosystem Track ($100,000 Prize Pool) / Developer Tooling & Infrastructure  
**Repository:** https://github.com/Ishant5436/sol-sec-proxy  
**License:** MIT (Permissive Open Source)  
**Author / Team:** Ishant Panchal (Solo Quantitative Systems Engineer, Somaiya Vidyavihar University)  
**Contact:** `ishant.p@somaiya.edu` / GitHub: `@Ishant5436`  

---

## 1. Executive Summary & Problem Space

On the Solana network, transactions that fail during on-chain execution—whether due to slippage violations in DEX CPIs, Anchor constraint violations, or expired blockhashes—**still consume 100% of their base and priority fees**.

For autonomous trading agents, high-frequency market makers, dApps, and retail wallets, this causes severe friction:
1. **Wasted Capital on Reverts:** In congested conditions, automated trading agents attach heavy priority fees (50,000 to 500,000 micro-lamports/CU). When an on-chain condition fails, the entire priority fee is burned, resulting in direct capital leakage with zero utility.
2. **Cryptic Debugging & Opaque Error Feedback:** Standard Solana RPC endpoints return opaque errors like `{"InstructionError": [2, {"Custom": 6000}]}` requiring tedious IDL lookups.
3. **Compute Unit (CU) Inefficiency:** Over-requesting static CU limits (e.g., 1,400,000 CU) forces users to overpay on priority fees, while under-requesting triggers immediate `ComputeBudgetExceeded` halts.

`sol-sec-proxy` is a high-performance, deterministic pre-execution reverse proxy written in pure Rust. It runs locally as a daemon or sidecar between dApps, wallets, or agent runtimes and the upstream Solana RPC cluster (Triton, Helius, QuickNode, or local test validator).

---

## 2. System Architecture

```
+-----------------------------------------------------------------------------------+
|               Autonomous Agent / dApp / Wallet Client                             |
+-----------------------------------------+-----------------------------------------+
                                          | JSON-RPC 2.0 (HTTP / Port 8545)
                                          v
+-----------------------------------------------------------------------------------+
|                                  sol-sec-proxy                                    |
|                                                                                   |
|  +-----------------------+ +------------------------+ +------------------------+  |
|  | Wire Deserializer     | | Pre-Flight Simulator   | | Anchor Error Decoder   |  |
|  | Zero-Copy v0 / Legacy | | Dynamic Blockhash Swp  | | Framework 100-103      |  |
|  | ALT Account Tables    | | SigVerify: false       | | Constraints 2000-2018  |  |
|  +----------+------------+ +-----------+------------+ +-----------+------------+  |
|             |                          |                          |               |
|             +--------------------------+--------------------------+               |
|                                        v                                          |
|                          Dynamic Compute Budget Optimizer                         |
|                          - Actual unitsConsumed Parsing                           |
|                          - Optimal CU Limit (+15% Headroom)                       |
|                          - Account Lock Contention Telemetry                      |
+-----------------------------------------+-----------------------------------------+
                                          | Transparent Forward / Intercept Revert
                                          v
+-----------------------------------------------------------------------------------+
|                     Upstream Solana RPC Cluster (Triton / Helius)                 |
+-----------------------------------------------------------------------------------+
```

### Core Technical Pillars

1. **Pre-Flight Execution Firewall (`sendTransaction` Interception):**  
   Intercepts serialized base58/base64 transactions prior to network broadcast. Dispatches a non-blocking `simulateTransaction` call with signature verification disabled and dynamic blockhash replacement. If the simulation reverts, the proxy halts propagation immediately, returning a structured JSON-RPC error directly to the caller, saving 100% of priority fees.
2. **Anchor & Native Error Decoder:**  
   Translates raw Solana instruction errors and custom program error codes into human-readable diagnostics with instruction index, failure category (`ConstraintRaw`, `ConstraintMut`, `RequireEqViolated`), and sanitized log excerpts.
3. **Dynamic Compute Budget Optimizer:**  
   Reads exact `unitsConsumed` during simulation, dynamically calculates optimal `SetComputeUnitLimit` parameters with configurable safety headroom (default +15%), and analyzes writable accounts to alert callers of lock contention risks.
4. **Deterministic Safety Standards:**  
   Engineered strictly under Mission-Critical Safety Invariants: bounded loops, zero dynamic allocation on hot paths, checked math, and minimum two assertions per function. Zero compiler and linter warnings under `#![deny(warnings)]`.

---

## 3. Empirical Performance Benchmarks

Benchmarked on Apple Silicon ARM64 using Criterion:

| Operation | Median Latency | Throughput | Allocation on Hot Path |
|---|---|---|---|
| Wire Deserialization (Legacy Tx) | 895.79 ns | 1,116,000 ops/sec | 0 bytes heap |
| Wire Deserialization (v0 + ALTs) | 1,240.12 ns | 806,000 ops/sec | 0 bytes heap |
| Anchor Error Decoding | 142.30 ns | 7,027,000 ops/sec | 0 bytes heap |
| Dynamic CU Limit Calculation | 84.10 ns | 11,890,000 ops/sec | 0 bytes heap |

*Verified: The entire interception and decoding pipeline adds less than 1.5 microseconds of processing overhead.*

---

## 4. Verification & Testing Evidence

- **Test Suite Pass Rate:** 19/19 unit and integration tests passing green (`cargo test`).
- **Static Analysis:** Zero compiler or Clippy warnings under `#![deny(warnings)]`.
- **Memory Safety:** 100% safe Rust without unhandled unwraps on public input paths.

### Reproduction Commands:
```bash
# Clone and build
git clone https://github.com/Ishant5436/sol-sec-proxy.git
cd sol-sec-proxy

# Execute full automated test suite
cargo test

# Launch proxy in release mode
UPSTREAM_RPC_URL="https://api.mainnet-beta.solana.com" PORT=8545 cargo run --release
```

---

## 5. Submission Details for Judges

- **Portal:** Colosseum Crypto World's Fair (`https://colosseum.com/worldsfair`)
- **Category:** Infrastructure & Developer Tooling
- **Primary Ecosystem:** Solana
- **Open Source:** Permissive MIT License
- **Deployment Status:** Standalone daemon + Docker container ready for local, sidecar, or edge deployment.
