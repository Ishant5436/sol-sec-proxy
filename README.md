# sol-sec-proxy

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Language: Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](Cargo.toml)
[![Architecture](https://img.shields.io/badge/Arch-Apple_Silicon_ARM64_%2F_Linux_x86__64-brightgreen.svg)]()
[![Invariants](https://img.shields.io/badge/Safety_Standards-Power_of_10-success.svg)]()

Deterministic, zero-overhead pre-execution transaction simulator, compute-budget optimizer, and Anchor error interceptor for Solana. Built in pure Rust.

---

## Why sol-sec-proxy Exists

On Solana, transactions that fail during on-chain execution—whether due to slippage violations in DEX CPIs, Anchor constraint failures, or compute budget exhaustion—**still consume 100% of their base and priority fees**.

For autonomous trading agents, high-frequency pipelines, and wallets, this leads to:
1. **Wasted Capital:** In congested conditions, heavy priority fees (50k–500k micro-lamports/CU) are burned on reverted transactions with zero on-chain utility.
2. **Cryptic Debugging:** Standard Solana RPC endpoints return opaque errors like `{"InstructionError": [2, {"Custom": 6000}]}` requiring tedious IDL lookups.
3. **Compute Unit Inefficiency:** Over-requesting static CU limits (e.g. 1.4M CU) causes overpayment on priority fees, while under-requesting causes `ComputeBudgetExceeded` halts.

`sol-sec-proxy` sits transparently between your dApp/agent and the upstream Solana RPC cluster, simulating transactions before network broadcast and decoding errors into human-readable diagnostics.

---

## Architecture

```
+------------------+         JSON-RPC          +--------------------+
|  Agent / Client  | -----------------------> |   sol-sec-proxy    |
| (dApp / Wallet)  | <----------------------- | (Rust / Hyper / CU)|
+------------------+     Intercept / Decode   +--------------------+
                                                         |
                                                Pre-flight Simulation
                                                & CU Optimization
                                                         v
                                              +--------------------+
                                              | Solana RPC Cluster |
                                              | (Triton / Helius)  |
                                              +--------------------+
```

### Core Features

* **Pre-Flight Execution Firewall (`sendTransaction` Interception):** Intercepts serialized transactions before broadcast. Executes non-blocking pre-flight simulation against upstream RPC with `sigVerify: false` and `replaceRecentBlockhash: true`. If simulation reverts, halts broadcast immediately and returns a structured JSON-RPC error, saving 100% of priority fees.
* **Anchor & Native Error Decoder:** Translates Solana instruction errors and custom program errors into structured diagnostics:
  * Framework errors (100–103)
  * Constraint violations (2000–2018: `ConstraintRaw`, `ConstraintMut`, `ConstraintSigner`, etc.)
  * Require expressions (2500–2505: `RequireEqViolated`, etc.)
  * Account lifecycle errors (3000–3007)
  * Custom program error numbers (6000+)
* **Dynamic Compute Budget Optimizer:** Measures actual `unitsConsumed` during simulation, calculates optimal `SetComputeUnitLimit` with configurable safety margin (+15%), and analyzes writable account locks.
* **Deterministic Safety Standards:** Adheres to Gerard J. Holzmann's Power of 10 Safety Invariants: bounded loops, checked arithmetic, minimum 2 assertions per function, zero dynamic memory on hot path.

---

## Quickstart

### 1. Build from Source

```bash
git clone https://github.com/Ishant5436/sol-sec-proxy.git
cd sol-sec-proxy
make build
```

### 2. Configure Environment

Copy `.env.example` to `.env` and set your upstream RPC URL:

```bash
cp .env.example .env
```

```env
SOLANA_UPSTREAM_RPC_URL=https://api.mainnet-beta.solana.com
PROXY_HOST=127.0.0.1
PROXY_PORT=8899
CU_SAFETY_BUFFER_PERCENT=15
MAX_CU_LIMIT=1400000
RPC_TIMEOUT_MS=5000
```

### 3. Run the Proxy Daemon

```bash
make run
```

The proxy will listen on `http://127.0.0.1:8899`. Point your `@solana/web3.js` Connection or agent RPC configuration to `http://127.0.0.1:8899`.

---

## Testing & Quality Gate

```bash
# Run all unit and integration tests
make test

# Run strict clippy static analysis and formatting check
make lint
```

All 18 unit tests pass with zero warnings:
```
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

---

## License

MIT License. Copyright (c) 2026 Ishant Panchal.
