# Contributing to Skillz

Thank you for your interest in contributing to Skillz! This document provides guidelines and instructions for contributing.

## 🚀 Getting Started

### Prerequisites

- **Rust 1.70+** with the `wasm32-wasip1` target
- **Git** for version control
- **Python 3** for running tests

### Setup

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/skillz.git
cd skillz/mcp-wasm-host

# Install WASM target
rustup target add wasm32-wasip1

# Build the project
cargo build

# Run tests
python3 test_e2e.py
```

## 🔧 Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### 2. Make Changes

- Follow the existing code style
- Add tests for new functionality
- Update documentation as needed

### 3. Test Your Changes

```bash
# Build
cargo build

# Run all tests
python3 test_e2e.py
python3 test_validate.py
python3 test_persistence.py
python3 test_workflow.py

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy
```

### 4. Commit Your Changes

Follow conventional commit messages:

```
feat: add new feature
fix: fix a bug
docs: update documentation
test: add tests
refactor: refactor code
```

### 5. Submit a Pull Request

- Push your branch to your fork
- Create a Pull Request against `main`
- Describe your changes clearly
- Link any related issues

## 📁 Project Structure

```
mcp-wasm-host/
├── src/
│   ├── main.rs       # MCP server, tools, resources
│   ├── builder.rs    # Rust → WASM compilation
│   ├── runtime.rs    # WASM & Script execution
│   └── registry.rs   # Tool storage & management
├── docs/             # GitHub Pages documentation
├── tools/            # Compiled tools directory
└── tests/            # Test files
```

## 🎯 Areas for Contribution

### High Priority

- [ ] Additional language examples (Go, Deno, PHP)
- [ ] Improved error messages
- [ ] Performance optimizations
- [ ] More comprehensive tests

### Features

- [ ] Tool versioning
- [ ] Tool dependencies
- [ ] Input/output schemas
- [ ] Tool marketplace

### Documentation

- [ ] More examples
- [ ] Video tutorials
- [ ] Architecture deep-dive
- [ ] Performance benchmarks

## 📝 Code Style

### Rust

- Use `rustfmt` for formatting
- Follow Rust API guidelines
- Document public APIs with doc comments
- Use meaningful variable names

### Documentation

- Use clear, concise language
- Include code examples
- Keep README.md up to date

## 🐛 Reporting Bugs

When reporting bugs, please include:

1. **Description**: Clear description of the bug
2. **Steps to Reproduce**: Minimal steps to reproduce
3. **Expected Behavior**: What should happen
4. **Actual Behavior**: What actually happens
5. **Environment**: OS, Rust version, etc.

## 💡 Feature Requests

For feature requests, please include:

1. **Problem**: What problem does this solve?
2. **Solution**: Proposed solution
3. **Alternatives**: Any alternatives considered
4. **Impact**: Who benefits from this?

## 📜 License

By contributing, you agree that your contributions will be licensed under the MIT License.

## 🙏 Thank You!

Every contribution, no matter how small, makes a difference. Thank you for helping make Skillz better!

