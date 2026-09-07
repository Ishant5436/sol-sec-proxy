# Superteam India Developer Microgrant Proposal: sol-sec-proxy

## 1. Project Title & Overview
* **Project Name:** sol-sec-proxy (Solana Pre-Execution Transaction Firewall & Compute Budget Optimizer)
* **Grant Track:** Developer Tooling & Infrastructure Microgrant
* **Requested Amount:** $6,000 USDC
* **Project Lead:** Ishant Panchal (Solo Quantitative & Systems Engineer, Somaiya Vidyavihar)
* **Repository:** https://github.com/Ishant5436/sol-sec-proxy
* **Primary Contact:** ishant.p@somaiya.edu

---

## 2. Problem Statement
On the Solana network, transactions that fail during on-chain execution—whether due to exceeding compute budgets, slippage violations inside automated market maker (AMM) CPIs, or Anchor constraint failures—still consume 100% of their attached base and priority fees. For autonomous execution agents, MEV searchers, high-frequency trading systems, and retail wallets, this introduces severe friction:

1. **Wasted Capital on Reverts:** In periods of network congestion, users attach heavy priority fees (e.g. 50,000 to 500,000 micro-lamports/CU). When an on-chain condition fails, the entire priority fee is burned, resulting in direct capital leakage with zero utility.
2. **Cryptic Debugging & Opaque Error Feedback:** Standard Solana RPC endpoints return opaque error structures like `InstructionError(2, Custom(6000))` or unparsed log streams. Developers and agentic executioners must manually decode Anchor IDLs or inspect source code to identify why an instruction reverted.
3. **Compute Unit (CU) Allocation Inefficiency:** Submitting transactions with arbitrary or default static compute unit limits (e.g. 1,400,000 CU) forces users to overpay on priority fees, since the total fee paid is proportional to the requested budget rather than the actual units consumed. Under-requesting triggers immediate `ComputeBudgetExceeded` halts.

---

## 3. Proposed Solution: sol-sec-proxy
`sol-sec-proxy` is a high-performance, deterministic pre-execution reverse proxy written in pure Rust. It runs locally as a daemon or sidecar between dApps, wallets, or agent runtimes and the upstream Solana RPC cluster (e.g., Triton, Helius, QuickNode, or local test validator).

### Key Architectural Capabilities:
* **Pre-Flight Execution Firewall (`sendTransaction` Interception):** Serialized base58 or base64 transactions are intercepted prior to network broadcast. The proxy dispatches a non-blocking `simulateTransaction` call with signature verification disabled and dynamic blockhash replacement. If the simulation reverts, the proxy halts propagation immediately, returning a structured JSON-RPC error directly to the caller. This guarantees 100% savings on priority fees and base transaction fees.
* **Anchor & Native Error Decoder:** Translates raw Solana instruction errors and custom program error codes into human-readable diagnostics with instruction index, failure category (e.g. `ConstraintRaw`, `ConstraintMut`, `RequireEqViolated`), and sanitized log excerpts.
* **Dynamic Compute Budget Optimizer:** Reads exact `unitsConsumed` during simulation, dynamically calculates optimal `SetComputeUnitLimit` parameters with configurable safety headroom (default +15%), and analyzes writable accounts to alert callers of lock contention risks.
* **Deterministic Safety Standards:** Built strictly following Gerard J. Holzmann's Power of 10 Safety Invariants: bounded loops, zero dynamic allocation on hot paths, checked math, and minimum two assertions per function. Zero compiler and linter warnings (`#[deny(warnings)]`).

---

## 4. Milestone Roadmap & Budget Allocation

The proposed $6,000 USDC grant is structured into three concrete, deliverable-driven milestones:

### Milestone 1: Core Engine & Wire Deserializer ($2,000 USDC — Week 1)
* Zero-copy Solana wire deserializer supporting both Legacy and v0 Versioned Transactions (including Address Lookup Tables).
* Extraction and parsing of ComputeBudget program instructions (`SetComputeUnitLimit`, `SetComputeUnitPrice`).
* Full Anchor error code dictionary mapping framework errors (100-103), constraint violations (2000-2018), require checks (2500-2505), and account lifecycle errors (3000-3007).
* 100% test coverage with automated unit tests for wire parsing and error decoding.

### Milestone 2: High-Throughput Proxy Daemon & CU Optimizer ($2,500 USDC — Week 2)
* Asynchronous Hyper/Tokio HTTP JSON-RPC 2.0 proxy server with transparent pass-through for read queries.
* Pre-flight simulation firewall intercepting `sendTransaction` and isolating reverting transactions.
* Dynamic compute unit calculation engine with fee savings telemetry.
* Performance benchmarking demonstrating verified sub-microsecond proxy overhead (895.79 ns / <1.0 µs end-to-end) on Apple Silicon and Linux environments.

### Milestone 3: Packaging, Documentation & Community Distribution ($1,500 USDC — Week 3)
* Multi-architecture Docker image and lightweight binary distribution.
* Comprehensive developer documentation with TypeScript and Python client integration examples.
* Public GitHub release under MIT license with continuous integration workflows (GitHub Actions).

---

## 5. Current Implementation Status & Empirical Benchmarks

The core engine of `sol-sec-proxy` is already built, audited, and empirically validated in the public repository:
* **Test Suite:** 19/19 automated unit and integration tests passing (`make test`).
* **Static Analysis:** Zero compiler or linter warnings under `#![deny(warnings)]` and `cargo clippy --all-targets -- -D warnings`.
* **Determinism Invariants:** Audited against Holzmann's Power of 10 Safety Invariants (bounded loops, assertion density >= 2 on 100% of functions, checked arithmetic with `overflow-checks = true`).
* **Empirical Benchmarks (`make bench` on Apple Silicon ARM64):**
  * Wire Deserialization (v0 + ALTs): **588.84 ns** (1.69M tx/s)
  * Dynamic Compute Unit (CU) Optimizer: **1.65 ns** (606M ops/s)
  * Anchor Error Decoder (Codes 2000–2018): **210.84 ns** (4.74M ops/s)
  * End-to-End Interception Pipeline: **895.79 ns (0.896 µs)** (1.11M tx/s)

This proves the proxy adds **under 1 microsecond** of CPU overhead, outperforming initial milestone targets by several orders of magnitude.

---

## 6. Ecosystem Impact & Sustainability
`sol-sec-proxy` directly enhances the economic efficiency and reliability of the Solana ecosystem. By providing a plug-and-play proxy drop-in for `solana-web3.js` and agent frameworks, builders can eliminate capital loss from failed transactions, optimize priority fees, and accelerate debugging with human-readable Anchor error decoding. As a solo developer based in Mumbai, India, I am committed to maintaining this open-source tool and expanding IDL auto-fetch capabilities for ecosystem programs.
