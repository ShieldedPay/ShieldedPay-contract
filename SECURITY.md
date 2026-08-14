# Security Policy

## Reporting a Vulnerability

We take the security of ShieldedPay Contracts seriously. If you discover a security vulnerability, please follow these steps:

1. **Do not** disclose the vulnerability publicly, and **do not** submit it as a normal GitHub issue
2. Email us at security@shieldedpay.com or open a private security advisory on GitHub
3. Include a description of the vulnerability, the affected contract (`treasury`, `payroll`, or `claim`), and steps to reproduce
4. Allow us reasonable time to address the issue before public disclosure

## What to Expect

- Acknowledgment of your report within 48 hours
- Regular updates on the progress of the fix
- Credit for the discovery if you wish

## Scope

This policy covers the Soroban smart contracts in this repository:

| Contract | Status |
|---|---|
| `treasury` | Has logic + a unit test |
| `payroll` | Scaffold only — no logic yet |
| `claim` | Scaffold only — no logic yet |

## Audit Status

**These contracts have not been audited.** They are under active development and should be treated as pre-audit / testnet-only. Do not deploy to Stellar mainnet or use to custody real funds until an independent audit has been completed and this section is updated with the result.

## Deployment

Current deployments (if any) are testnet-only. Mainnet contract addresses, once they exist, will be listed here and in the README — this file should be the source of truth for deployment/audit status, kept current as the project matures.

## Supported Versions

| Version | Supported          |
|---------|---------------------|
| 0.1.x   | :white_check_mark: |
