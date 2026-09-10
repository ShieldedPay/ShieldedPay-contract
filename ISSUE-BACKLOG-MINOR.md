# ShieldedPay-contract — Minor Issues Backlog (9 Issues)

Sized strictly as **100 pts (Trivial / Good First Issue)** in Drips Wave criteria.

---

## #1: Add docstrings and inline comments explaining merkle leaf domain separation in payroll

- **Labels**: `complexity:trivial, contract, documentation, good first issue`

- **Complexity**: `100 pts` (Trivial)


### Summary
Add comprehensive Rust docstrings and inline comments detailing the `0x00` (leaf) vs `0x01` (branch) domain separation hashing mechanism in `payroll/src/lib.rs`.

### Requirements
- Add docstrings explaining preimage attack mitigation to `compute_leaf_hash` and `verify_merkle_proof`.
- Document expected hash output lengths and endianness conventions.
- Verify `cargo doc --no-deps` generates clean documentation without warnings.

---

## #2: Add Soroban SDK version, build status, and Apache-2.0 license badges to README

- **Labels**: `complexity:trivial, contract, documentation`

- **Complexity**: `100 pts` (Trivial)


### Summary
Enhance the repository README with clear status badges for Soroban SDK version, test suite status, and licensing.

### Requirements
- Add Shields.io badges for Rust edition, Soroban SDK v22, and Apache-2.0 license.
- Ensure all badge links point to valid GitHub Actions workflow runs and repository files.
- Verify markdown rendering is formatted cleanly.

---

## #3: Add unit test validating rejection of zero-amount payroll voucher claims

- **Labels**: `complexity:trivial, contract, testing, good first issue`

- **Complexity**: `100 pts` (Trivial)


### Summary
Ensure the payroll smart contract explicitly rejects voucher claims specifying a zero or negative token disbursement amount.

### Requirements
- Add test case in `tests/payroll_tests.rs` attempting to execute `claim` with `amount = 0`.
- Assert contract returns `Error::InvalidAmount`.
- Verify full test suite passes with `cargo test`.

---

## #4: Add unit test verifying rejection of claims with expired voucher timestamps

- **Labels**: `complexity:trivial, contract, testing`

- **Complexity**: `100 pts` (Trivial)


### Summary
Validate that the claim verification logic properly enforces time-lock expiration using `env.ledger().timestamp()`.

### Requirements
- Add test in `tests/claim_tests.rs` manipulating ledger timestamp past expiration window.
- Confirm contract call reverts with `Error::VoucherExpired`.
- Ensure test passes deterministically.

---

## #5: Extract hardcoded constants to dedicated types.rs module with docstrings

- **Labels**: `complexity:trivial, contract, code-hygiene`

- **Complexity**: `100 pts` (Trivial)


### Summary
Extract magic number literals (e.g., maximum leaves per tree, default expiration duration) into named, documented constants.

### Requirements
- Create or update `types.rs` with `pub const MAX_BATCH_EMPLOYEES: u32 = 1024;` and `pub const DEFAULT_EXPIRY_DAYS: u64 = 90;`.
- Replace hardcoded literals across `payroll` and `treasury` crates with imported constants.
- Ensure 100% compilation and test pass rate.

---

## #6: Add custom error variant docstrings in errors.rs for client SDK readability

- **Labels**: `complexity:trivial, contract, documentation, good first issue`

- **Complexity**: `100 pts` (Trivial)


### Summary
Improve developer experience by providing informative docstrings on every variant of `ContractError` in `errors.rs`.

### Requirements
- Annotate each enum variant (`InvalidProof`, `BatchExpired`, `AlreadyClaimed`, etc.) with docstrings explaining cause and fix.
- Verify generated contract metadata includes error documentation.

---

## #7: Add cargo clippy and fmt check scripts to Makefile and CI workflow

- **Labels**: `complexity:trivial, contract, ci, tooling`

- **Complexity**: `100 pts` (Trivial)


### Summary
Standardize static analysis by adding automated `cargo clippy -- -D warnings` and `cargo fmt --check` commands to project build tooling.

### Requirements
- Add `lint` and `fmt-check` targets to `Makefile`.
- Ensure zero warnings are emitted on current codebase.
- Document command usage in `CONTRIBUTING.md`.

---

## #8: Add contract function gas consumption benchmarks report in BENCHMARKS.md

- **Labels**: `complexity:trivial, contract, documentation`

- **Complexity**: `100 pts` (Trivial)


### Summary
Benchmark and record CPU and memory fuel consumption for `deposit`, `create_batch`, and `claim` contract invocations.

### Requirements
- Run Soroban test budget meter on key workflows.
- Create `BENCHMARKS.md` tabulating CPU instructions and memory byte consumption.
- Provide instructions for reproducing benchmark results locally.

---

## #9: Add clean interface summary markdown table for Treasury and Payroll in README

- **Labels**: `complexity:trivial, contract, documentation`

- **Complexity**: `100 pts` (Trivial)


### Summary
Provide a developer-friendly overview table of public contract endpoints, parameters, and return types in `README.md`.

### Requirements
- Tabulate all entry points for `Treasury` contract (`initialize`, `deposit`, `withdraw`, `get_balance`).
- Tabulate all entry points for `Payroll` contract (`register_batch`, `claim_voucher`, `reclaim_expired`).
- Ensure all parameter types match Soroban SDK v22 types.

---
