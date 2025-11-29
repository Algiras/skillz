# Contributing to Skillz

Thank you for your interest in contributing to Skillz! This document provides guidelines and instructions for contributing.

## 🚀 Getting Started

### Prerequisites

- **Rust 1.70+** with the `wasm32-wasip1` target
- **Git** for version control

### Setup

```bash
# Clone the repository
git clone https://github.com/Algiras/skillz.git
cd skillz

# Install WASM target
rustup target add wasm32-wasip1

# Build the project
cargo build

# Run tests
cargo test
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
cargo test

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --all-targets -- -D warnings
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
- Create a Pull Request against `master`
- Describe your changes clearly
- Link any related issues

## 📁 Project Structure

```
skillz/
├── src/
│   ├── main.rs       # MCP server, tools, resources
│   ├── builder.rs    # Rust → WASM compilation
│   ├── runtime.rs    # WASM & Script execution
│   └── registry.rs   # Tool storage & management
├── tests/            # Integration tests
├── docs/             # GitHub Pages documentation
└── .github/          # CI/CD workflows
```

## 📜 Script Tool Best Practices

When writing script tools, follow these critical guidelines:

### ⚠️ Use `readline()` NOT `read()`

```python
# ✅ CORRECT - Returns immediately after reading request
request = json.loads(sys.stdin.readline())

# ❌ WRONG - Blocks forever waiting for EOF!
request = json.loads(sys.stdin.read())
```

### Always Flush Output

```python
print(json.dumps(response))
sys.stdout.flush()  # Important!
```

### Extract Arguments Correctly

```python
request = json.loads(sys.stdin.readline())
args = request.get('params', {}).get('arguments', {})
context = request.get('params', {}).get('context', {})
```

### Use Proper JSON-RPC Response Format

```python
response = {
    "jsonrpc": "2.0",
    "result": {"your": "data"},
    "id": request.get("id")  # Include request ID!
}
```

## 🎯 Areas for Contribution

### High Priority

- [ ] Additional language examples (Go, Deno, PHP)
- [ ] Improved error messages
- [ ] Performance optimizations
- [ ] More comprehensive tests
- [ ] Tool result streaming (large outputs)
- [ ] HTTP transport with SSE

### Completed Features ✅

- [x] Tool dependencies (pip/npm)
- [x] Input/output schemas
- [x] Tool annotations
- [x] Completion API
- [x] Code execution mode

### Future Features

- [ ] Tool versioning
- [ ] Tool marketplace
- [ ] Better sandbox isolation (gVisor, Firecracker)

### Documentation

- [ ] More examples
- [ ] Video tutorials
- [ ] Architecture deep-dive
- [ ] Performance benchmarks

## 📝 Code Style

### Rust

- Use `rustfmt` for formatting: `cargo fmt`
- Use `clippy` for linting: `cargo clippy`
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
