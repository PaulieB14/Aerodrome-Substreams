# Contributing to Aerodrome Substreams

Thank you for your interest in contributing to Aerodrome Substreams!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/Aerodrome-Substreams.git`
3. Create a feature branch: `git checkout -b feature/your-feature-name`
4. Make your changes
5. Test your changes: `cargo test`
6. Commit your changes: `git commit -m "Add your feature"`
7. Push to your fork: `git push origin feature/your-feature-name`
8. Create a Pull Request

## Development Setup

### Prerequisites
- Rust (latest stable)
- Substreams CLI
- Base network access

### Building
```bash
cargo build --release --target wasm32-unknown-unknown
```

### Testing
```bash
cargo test
substreams run substreams.yaml map_blocks --start-block 1000000 --stop-block +10
```

## Code Style

- Follow Rust conventions
- Add documentation for public functions
- Include tests for new features
- Use meaningful commit messages

## Issues

- Use GitHub Issues for bug reports
- Provide clear reproduction steps
- Include relevant logs and error messages

## Pull Requests

- Keep PRs focused and small
- Include tests for new features
- Update documentation as needed
- Ensure all tests pass

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
