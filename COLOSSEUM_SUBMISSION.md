# Colosseum Crypto World's Fair 2026: Project Submission Dossier

**Project Name:** sol-sec-proxy
**Tagline:** Pre-Execution Failure Diagnostics & Compute-Budget Advisory for Skip-Preflight Solana Agents
**Track:** Solana Ecosystem Track ($100,000 Prize Pool) / Developer Tooling & Infrastructure
**Repository:** https://github.com/Ishant5436/sol-sec-proxy
**License:** MIT (Permissive Open Source)
**Author / Team:** Ishant Panchal (Solo Quantitative Systems Engineer, Somaiya Vidyavihar University)
**Contact:** `ishant.p@somaiya.edu` / GitHub: `@Ishant5436`
**Submission Deadline:** October 12, 2026, 11:59 PM PT

---

## 1. Executive Summary & Problem Space

Solana's `sendTransaction` RPC method already runs a preflight `simulateTransaction` by default (`skipPreflight: false`) on every major RPC provider (Triton, Helius, QuickNode). That default preflight is free protection against most reverts, but it costs an extra round trip before broadcast.

For that reason, autonomous trading agents, market makers, and other latency-sensitive callers routinely set `skipPreflight: true` to shave that round trip off the hot path. Doing so trades away the one thing preflight gave them: a decoded reason when a transaction would have failed. What they get back instead, when a transaction does revert on-chain, is a raw, opaque error such as `{"InstructionError": [2, {"Custom": 6000}]}`, which requires a manual IDL lookup to interpret.

Separately, callers that pick a static Compute Unit (CU) limit face the same over/under-request tradeoff regardless of preflight: request too many CUs and overpay on priority fees; request too few and risk a `ComputeBudgetExceeded` halt.

`sol-sec-proxy` is a pure-Rust, deterministic decode-and-advisory layer that sits alongside the upstream Solana RPC cluster and gives skip-preflight callers back what they gave up: a structured, human-readable failure diagnosis and a right-sized CU recommendation, without forcing every transaction back onto the full preflight round trip. It is not a replacement for `simulateTransaction`, it is the decode/advisory layer a team would otherwise have to hand-write on top of it.

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
|                          Compute Budget Advisory Engine                           |
|                          - Actual unitsConsumed Parsing                           |
|                          - Recommended CU Limit (+15% Headroom)                    |
|                          - Account Lock Contention Telemetry                       |
+-----------------------------------------+-----------------------------------------+
                                          | Transparent Forward / Structured Diagnosis
                                          v
+-----------------------------------------------------------------------------------+
|                     Upstream Solana RPC Cluster (Triton / Helius)                 |
+-----------------------------------------------------------------------------------+
```

### Core Technical Pillars

1. **Pre-Flight Simulation & Structured Diagnosis (`sendTransaction` Interception):**
   Intercepts serialized base58/base64 transactions prior to network broadcast. Dispatches a non-blocking `simulateTransaction` call with signature verification disabled and dynamic blockhash replacement. If the simulation reverts, the proxy halts propagation immediately and returns a structured JSON-RPC error to the caller, so a caller who normally runs with `skipPreflight: true` can opt into this path when it wants a decoded diagnosis instead of a raw revert.
2. **Anchor & Native Error Decoder:**
   Translates raw Solana instruction errors and custom program error codes into human-readable diagnostics with instruction index, failure category (`ConstraintRaw`, `ConstraintMut`, `RequireEqViolated`), and sanitized log excerpts.
3. **Compute Budget Advisory:**
   Reads exact `unitsConsumed` during simulation and returns a `CuOptimizationRecommendation` with a suggested `SetComputeUnitLimit` value (default +15% safety headroom) and writable-account lock-contention telemetry. This is advisory only: the recommendation is returned to the caller to apply on its *next* transaction build. It cannot and does not rewrite an already-signed transaction in flight.
4. **Deterministic Safety Standards:**
   Engineered strictly under Mission-Critical Safety Invariants: bounded loops, zero dynamic allocation on hot paths, checked math, and minimum two assertions per function. Zero compiler and linter warnings under `#![deny(warnings)]`.

---

## 3. Empirical Performance Benchmarks

Benchmarked on Apple Silicon ARM64 using Criterion, 100,000 iterations per subsystem (full results and methodology in [BENCHMARKS.md](BENCHMARKS.md)):

| Operation | Median Latency | Throughput | Allocation on Hot Path |
|---|---|---|---|
| Wire Deserialization (v0 + ALTs) | 588.84 ns | 1,698,263 ops/sec | 0 bytes heap |
| Compute Budget Advisory Calculation | 1.65 ns | 606,214,915 ops/sec | 0 bytes heap |
| Anchor Error & Log Decoding | 210.84 ns | 4,742,914 ops/sec | 0 bytes heap |
| End-to-End Interception Pipeline | 895.79 ns | 1,116,331 ops/sec | 0 bytes heap |

*These figures are the internal decode/advisory-computation overhead only, measured in-process with `std::hint::black_box`. They do not include the network round trip to the upstream RPC for `simulateTransaction`, which is on the order of tens of milliseconds and dominates real-world end-to-end latency. The claim here is that sol-sec-proxy's own processing adds under one microsecond on top of that RPC call, not that the full request completes in under one microsecond.*

---

## 4. Prior Art & What This Adds

- **`simulateTransaction` (Solana RPC):** already provides preflight simulation; this is what `skipPreflight: true` callers deliberately forgo, and what sol-sec-proxy calls under the hood. sol-sec-proxy does not replace it, it wraps it with structured decoding.
- **`@coral-xyz/anchor` client-side error parsing:** can decode Anchor `AnchorError`s once you have the full IDL loaded client-side in TypeScript/JS. sol-sec-proxy decodes at the proxy layer in Rust, independent of any specific client language or IDL-loading step, and adds native/system program error categories alongside Anchor's.
- **`@solana/web3.js` CU estimation helpers:** exist for computing a request budget ahead of signing, but require the caller to already know `unitsConsumed`, typically from its own separate simulation call. sol-sec-proxy performs the simulation and returns the CU recommendation together, in one round trip.
- **What's new here:** bundling decoded, structured diagnostics and CU advisory into a single call, aimed specifically at agent/bot pipelines that already skip default preflight for latency and would otherwise have to hand-roll their own error-code and CU-estimation layer to get equivalent visibility.

---

## 5. Potential Impact & Total Addressable Market

Every Solana trading bot, market maker, or agent framework that sets `skipPreflight: true` today is a candidate user: none of them get decoded revert diagnostics for free, and all of them either hand-roll an Anchor error decoder or ship blind to `{"Custom": 6000}`-style errors. Solana's own agentic-payments push (x402-style agent-to-agent transaction flows) is growing this population specifically, since autonomous agents are the callers most likely to prioritize latency over the default safety net. A conservative near-term target is the long tail of independent trading-bot and MEV-adjacent teams building on Solana who do not have in-house infra teams to build this themselves; a longer-term target is agent-framework maintainers who could embed this as a dependency rather than have every downstream agent re-implement it.

---

## 6. User Experience (For Downstream Bot/Agent Developers)

Integration is a one-line RPC endpoint swap: point an existing `@solana/web3.js` `Connection` or equivalent client at `http://127.0.0.1:8899` instead of the upstream RPC URL, with no changes to transaction-building code. On a clean simulation, the call forwards transparently. On a revert, instead of the caller's own code needing to pattern-match `{"InstructionError": [...]}` and cross-reference an IDL by hand, it receives a JSON object naming the failing instruction index, the decoded constraint or error category, and (when relevant) a CU recommendation, all in one response it can log, alert on, or feed back into a self-correcting retry loop. The bar for "good UX" here is: does a developer have to write less error-handling code after adopting this than before, and the decoder's structured output is built directly toward that, not toward a human-facing dashboard.

---

## 7. Business Plan

This is not a one-off hackathon tool; the path to a sustainable product is a hosted, pay-per-call version of the same decode/advisory logic, offered as a drop-in RPC endpoint for agent frameworks and trading-bot platforms that don't want to run and maintain their own proxy instance. Comparable precedent exists in the ecosystem's move toward metered, agent-facing payment rails (x402-style micropayment gateways for API access), which is a natural billing model for a per-call diagnostic service consumed primarily by autonomous agents rather than humans. The self-hosted, MIT-licensed daemon in this repository remains free and open source; the monetization path is a managed multi-tenant deployment with usage-based pricing, aimed at teams that would rather pay per decoded revert than operate the infrastructure themselves. Solo-founder execution risk is real and is mitigated by the scope: this is a narrow, well-defined decode/advisory layer, not a platform requiring an ongoing roadmap of new surface area to stay useful.

---

## 8. Verification & Testing Evidence

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

## 9. Submission Details for Judges

- **Portal:** Colosseum Crypto World's Fair (`https://colosseum.com/worldsfair`)
- **Submission Deadline:** October 12, 2026, 11:59 PM PT
- **Category:** Infrastructure & Developer Tooling
- **Primary Ecosystem:** Solana
- **Open Source:** Permissive MIT License
- **Deployment Status:** Standalone daemon + Docker container ready for local, sidecar, or edge deployment.
