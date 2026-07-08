# Contributing to ShieldedPay Contracts

Thank you for your interest in contributing to ShieldedPay Contracts! We welcome contributions from the community.

## Getting Started

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/amazing-feature`)
3. Make your changes following existing conventions
4. Run `cargo build --workspace` and ensure no errors
5. Commit with a descriptive message
6. Push and open a Pull Request against `main`

## Guidelines

- **Keep PRs focused** — one feature or fix per PR
- **Add tests** for new contract logic
- **Mark placeholder functions** with `// TODO: Implement`
- **Follow Rust conventions** — use `cargo fmt` and `cargo clippy`
- **Update documentation** when adding or changing features

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy --workspace -- -D warnings` to check for lint issues
- Use meaningful variable and function names
- Add doc comments for public APIs

## Commit Messages

Use conventional commit format:

- `feat: add new feature`
- `fix: correct bug`
- `docs: update documentation`
- `chore: maintenance tasks`
- `refactor: code restructuring`

## Reporting Issues

- Check existing issues before creating a new one
- Provide a clear description of the problem
- Include steps to reproduce when applicable
- Suggest a solution if you have one

## Questions?

Open a discussion or issue for any questions about contributing.
