# ShieldedPay Contracts 🔒

**Smart contracts for privacy-first global payroll on Stellar.**

This repository contains the Soroban smart contracts that power ShieldedPay's on-chain operations: treasury management, payroll commitment verification, and payment claims.

---

## Contracts

| Contract | Description |
|----------|-------------|
| **Treasury** | Manages deposits, withdrawals, and BENJI yield strategy allocation |
| **Payroll** | Stores Merkle root commitments and verifies batch payroll integrity |
| **Claim** | Handles individual payment claims with nullifier-based double-spend prevention |

---

## Related Repositories

- [ShieldedPay Frontend](https://github.com/ShieldedPay/shieldedPay-frontend)
- [ShieldedPay Backend](https://github.com/ShieldedPay/ShieldedPay-backend)

---

## Getting Started

### Prerequisites

- Rust toolchain
- Stellar CLI / Soroban CLI

### Build

```bash
cargo build --workspace
```

### Deploy

```bash
./scripts/deploy.sh testnet
./scripts/deploy.sh mainnet
```

---

## Contributing

1. Fork the repo and create a feature branch (`git checkout -b feat/amazing-feature`).
2. Make your changes following existing conventions.
3. Run `cargo build --workspace` and ensure no errors.
4. Commit with a descriptive message.
5. Push and open a Pull Request against `main`.

### Guidelines

- Keep PRs focused -- one feature or fix per PR.
- Add tests for new contract logic.
- Mark placeholder functions with `// TODO: Implement`.

---

## Status

**MVP / Prototype.** These are scaffolded contract stubs. Real ZK-SNARK verification logic, nullifier management, and Stellar Soroban integration are pending implementation. Not audited for production use.
