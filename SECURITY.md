# Security Model

Skillz implements a layered security approach for tool execution.

## WASM Tools (Rust)

WASM tools run in a **fully sandboxed WebAssembly environment** via Wasmtime:

- **Memory Isolation**: Each WASM module has its own linear memory, isolated from the host
- **No File System Access**: By default, WASM tools cannot access the file system
- **No Network Access**: WASM tools cannot make network requests
- **Capability-Based Security**: Only explicitly granted capabilities are available
- **Deterministic Execution**: Same inputs produce same outputs

### Wasmtime Security Features

- Bounds-checking on all memory accesses
- Stack overflow protection
- No direct access to host memory or functions
- Sandboxed WASI implementation

## Script Tools (Python, Node.js, etc.)

Script tools run as **separate processes** with several security measures:

### Default Security

- **Filtered Environment**: Only safe environment variables are passed (HOME, USER, LANG, PATH, TERM)
- **Execution Context**: Scripts receive structured context via JSON-RPC, not direct access
- **Process Isolation**: Each script runs in its own process

### Optional Sandbox Mode

For enhanced security, Skillz supports Linux sandboxing tools:

#### Bubblewrap (bwrap)

```bash
# Enable bubblewrap sandboxing
export SKILLZ_SANDBOX=bubblewrap
```

Features:
- Linux namespace isolation (PID, network, mount, user)
- Read-only bind mounts for system directories
- Workspace roots are writable
- Network disabled by default

#### Firejail

```bash
# Enable firejail sandboxing
export SKILLZ_SANDBOX=firejail
```

Features:
- Seccomp system call filtering
- Private /tmp
- No root privileges
- Memory and time limits
- Whitelist-based file access

#### Nsjail

```bash
# Enable nsjail sandboxing (most restrictive)
export SKILLZ_SANDBOX=nsjail
```

Features:
- Comprehensive namespace isolation
- Cgroup resource limits
- Strict capability dropping
- Suitable for multi-tenant environments

### Enabling Network in Sandbox

```bash
# Allow network access in sandboxed scripts
export SKILLZ_SANDBOX_NETWORK=1
```

## Dependency Management Security

### Virtual Environments

- Python tools use isolated `venv` environments
- Node.js tools use isolated `node_modules` directories
- Dependencies are installed per-tool, not globally
- Environment paths are stored in tool configuration

### Risks

- Dependencies are installed from public registries (PyPI, npm)
- Supply chain attacks are possible
- Review dependencies before adding them

## Best Practices

### For Tool Authors

1. **Validate all inputs** - Never trust arguments blindly
2. **Use minimal dependencies** - Fewer deps = smaller attack surface
3. **Handle errors gracefully** - Don't leak sensitive info in errors
4. **Follow least privilege** - Request only needed capabilities

### For Server Operators

1. **Use sandbox mode on untrusted tools**
   ```bash
   export SKILLZ_SANDBOX=firejail  # or bubblewrap, nsjail
   ```

2. **Restrict workspace roots**
   - Only add directories the AI actually needs
   - Avoid adding home directory or system paths

3. **Review tools before enabling**
   - Check `~/tools/manifest.json` for registered tools
   - Review script code in `~/tools/scripts/`

4. **Monitor resource usage**
   - WASM tools have bounded memory
   - Script tools can be limited via sandbox

5. **Keep Skillz updated**
   ```bash
   cargo install skillz --force
   ```

## Threat Model

### Trusted

- The MCP client (Cursor, Claude Desktop, etc.)
- The user approving tool creation

### Semi-Trusted

- AI-generated code (reviewed by user before execution)
- Registered tools (persisted and can be audited)

### Untrusted

- External dependencies (pip/npm packages)
- Network responses (when network is enabled)

## Reporting Security Issues

Please report security vulnerabilities via GitHub Security Advisories:
https://github.com/Algiras/skillz/security/advisories

Do not open public issues for security vulnerabilities.

## Security Comparison

| Feature | WASM Tools | Script Tools (Default) | Script Tools (Sandbox) |
|---------|------------|------------------------|------------------------|
| Memory Isolation | ✅ Full | ❌ Process only | ✅ Namespace |
| File System | ❌ None | ⚠️ Limited to roots | ✅ Whitelist |
| Network | ❌ None | ⚠️ Full access | ✅ Disabled |
| System Calls | ✅ WASI only | ❌ All allowed | ✅ Seccomp |
| Resource Limits | ✅ Wasmtime | ❌ None | ✅ Cgroups |
