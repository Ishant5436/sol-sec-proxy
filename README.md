# sol-sec-proxy

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Language: Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](Cargo.toml)
[![Architecture](https://img.shields.io/badge/Arch-Apple_Silicon_ARM64_%2F_Linux_x86__64-brightgreen.svg)]()
[![Invariants](https://img.shields.io/badge/Safety_Standards-Power_of_10-success.svg)]()

Deterministic pre-execution transaction decoder, compute-budget advisory, and Anchor error interceptor for Solana. Built in pure Rust; sub-microsecond internal processing overhead on top of the upstream RPC's own simulation round trip.

---

## Why sol-sec-proxy Exists

Solana's `sendTransaction` already runs a preflight `simulateTransaction` by default (`skipPreflight: false`) on every major RPC provider, which is free protection against most reverts at the cost of an extra round trip. Autonomous trading agents and other latency-sensitive callers routinely set `skipPreflight: true` to shave off that round trip, trading away the one thing preflight gave them: a decoded reason when a transaction would have failed. What they get back on a real revert instead is a raw, opaque error like `{"InstructionError": [2, {"Custom": 6000}]}`, requiring a manual IDL lookup.

Separately, static CU limits force the same tradeoff regardless of preflight: over-requesting (e.g. 1.4M CU) overpays on priority fees, while under-requesting risks a `ComputeBudgetExceeded` halt.

`sol-sec-proxy` sits transparently between your dApp/agent and the upstream Solana RPC cluster. It is not a replacement for `simulateTransaction`; it's the decode-and-advisory layer skip-preflight callers would otherwise have to hand-roll, giving them back structured, human-readable failure diagnostics and a right-sized CU recommendation without forcing every transaction back onto the full preflight round trip.

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

* **Pre-Flight Simulation & Structured Diagnosis (`sendTransaction` Interception):** Intercepts serialized transactions before broadcast. Executes non-blocking pre-flight simulation against upstream RPC with `sigVerify: false` and `replaceRecentBlockhash: true`. If simulation reverts, halts broadcast immediately and returns a structured, decoded JSON-RPC error instead of the raw revert a `skipPreflight: true` caller would otherwise see.
* **Anchor & Native Error Decoder:** Translates Solana instruction errors and custom program errors into structured diagnostics:
  * Framework errors (100–103)
  * Constraint violations (2000–2018: `ConstraintRaw`, `ConstraintMut`, `ConstraintSigner`, etc.)
  * Require expressions (2500–2505: `RequireEqViolated`, etc.)
  * Account lifecycle errors (3000–3007)
  * Custom program error numbers (6000+)
* **Compute Budget Advisory:** Measures actual `unitsConsumed` during simulation and returns a recommended `SetComputeUnitLimit` with configurable safety margin (+15%) plus writable-account lock analysis. This is advisory only, returned to the caller to apply on its next transaction build; it cannot rewrite an already-signed transaction.
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

## Agent-Actionable Failure Diagnostics

When pre-flight simulation fails, the JSON-RPC error `data` now carries an additive `diagnosis` object next to the existing fields (`category`, `errorName`, `instructionIndex`, `avoidedWastedFeeLamports`, `logs`, which are unchanged). It is built offline from the simulation `err` value and logs; no IDLs or extra network calls are involved.

| Field | Meaning |
| --- | --- |
| `source` | `"transaction"` or `"instruction"` |
| `kind` | Variant name (for example `BlockhashNotFound`, `Custom`), or `Unknown(<raw json>)` |
| `instruction_index` | Failing instruction, when the error carries one |
| `program_id` / `program_name` | Program of the failing instruction (from `account_keys[program_id_index]`, falling back to the last `Program <id> failed:` log line). Names are set only for System, SPL Token, Token-2022, Associated Token Account, Compute Budget and Memo v2 |
| `custom_error` | `code`, `name`, `message`, `account`, parsed from Anchor log lines or `custom program error: 0x..` |
| `retry_class` | `RebuildWithFreshBlockhash`, `AlreadyLanded`, `NeedsFunds`, `RaiseComputeLimit`, `FixInputs`, `RetryLater`, `Fatal` or `Unknown` |
| `suggested_action` | One sentence for the caller |
| `logs_snippet` | Up to 5 failure-related log lines |

Example `diagnosis` for an Anchor error (produced by the test suite's decoder, not hand-written):

```json
{
  "source": "instruction",
  "kind": "Custom",
  "instruction_index": 0,
  "program_id": "Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS",
  "program_name": null,
  "custom_error": {
    "code": 6001,
    "name": "SlippageExceeded",
    "message": "Price slippage limit exceeded.",
    "account": null
  },
  "retry_class": "FixInputs",
  "suggested_action": "Read the program error name and message, then correct the instruction inputs. Program error: SlippageExceeded (6001).",
  "logs_snippet": [
    "Program log: AnchorError thrown in programs/dex/src/instructions/swap.rs:88. Error Code: SlippageExceeded. Error Number: 6001. Error Message: Price slippage limit exceeded.",
    "Program Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS failed: custom program error: 0x1771"
  ]
}
```

Limits: `retry_class` is a heuristic per error variant (see the reasoning comments in `src/retry.rs`), not a guarantee. Program-specific error names are only reported when the logs contain them. Unrecognised variants are reported as `Unknown(...)` and never crash the proxy.

---

## Testing & Quality Gate

```bash
# Run all unit and integration tests
make test

# Run strict clippy static analysis and formatting check
make lint
```

All 94 tests pass with zero warnings:
```
test result: ok. (summed across unit and integration test binaries)
```

---

## Performance & Microbenchmarks

`sol-sec-proxy` is engineered for ultra-low latency execution environments. Benchmarks were executed on Apple Silicon ARM64 with full release optimizations (`opt-level = 3`, `lto = true`, `overflow-checks = true`):

```bash
make bench
```

### Empirical Results (100,000 iterations / suite):

| Subsystem / Operation | Mean Latency | Throughput |
| :--- | :--- | :--- |
| **1. Wire Deserialization (v0 + Address Lookup Tables)** | 588.84 ns | 1,698,263 tx/s |
| **2. Compute Budget Advisory Calculation** | 1.65 ns | 606,214,915 ops/s |
| **3. Anchor Error Decoder (Codes 2000–2018)** | 210.84 ns | 4,742,914 ops/s |
| **4. End-to-End Interception Pipeline** | **895.79 ns (0.896 µs)** | **1,116,331 tx/s** |

These are internal decode/advisory-computation figures only (measured in-process, excluding the network round trip to the upstream RPC's `simulateTransaction` call, which is on the order of tens of milliseconds and dominates real end-to-end latency). The claim is that sol-sec-proxy's own processing adds **under 1 microsecond** on top of that RPC call, not that the full request completes in under one microsecond. Full benchmark details available in [BENCHMARKS.md](file:///Users/ishantpanchal/sol-sec-proxy/BENCHMARKS.md).

---

## License

MIT License. Copyright (c) 2026 Ishant Panchal.
