# Bridge Binary & Hook Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Python hook.py with a compiled Rust bridge binary, add hook config auto-repair, enrich events with terminal environment info, and add an OpenCode JS plugin.

**Architecture:** A separate Rust binary `vibe-bridge` reads JSON from stdin, enriches it with terminal environment variables, and sends it to the Unix socket. The main Tauri app auto-repairs hook configs on startup. An OpenCode JS plugin connects directly to the socket without needing the bridge.

**Tech Stack:** Rust (bridge binary), Tokio (async socket), serde_json (JSON), JavaScript (OpenCode plugin)

---

## File Structure

| File | Action | Purpose |
|------|--------|---------|
| `src-bridge/Cargo.toml` | Create | Bridge binary crate config |
| `src-bridge/src/main.rs` | Create | Bridge binary: stdin → enrich → socket |
| `src-tauri/src/claude/mod.rs` | Modify | Auto-repair hooks on startup, use bridge binary path |
| `src-tauri/src/claude/hooks.rs` | Create | Extracted hook setup/repair logic |
| `src-tauri/Cargo.toml` | Modify | Add build dependency to build bridge |
| `scripts/opencode-plugin.js` | Create | OpenCode JS plugin for direct socket |
| `src/types/index.ts` | Modify | Add terminal env fields to Session |
| `src-tauri/src/claude/mod.rs` | Modify | Session struct gets env fields |

---

### Task 1: Create the Rust Bridge Binary

**Files:**
- Create: `src-bridge/Cargo.toml`
- Create: `src-bridge/src/main.rs`

The bridge binary replaces hook.py. It reads JSON from stdin, enriches it with terminal environment variables (TERM_PROGRAM, ITERM_SESSION_ID, TMUX, TMUX_PANE, __CFBundleIdentifier), connects to the Unix socket, sends the payload, and optionally waits for a response (for blocking PreToolUse events).

- [ ] **Step 1: Create src-bridge/Cargo.toml**

```toml
[package]
name = "vibe-bridge"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "vibe-bridge"
path = "src/main.rs"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

- [ ] **Step 2: Create src-bridge/src/main.rs**

The binary should:
1. Read JSON from stdin
2. Parse it as a mutable serde_json::Value
3. Enrich with env vars: TERM_PROGRAM, ITERM_SESSION_ID, TMUX, TMUX_PANE, __CFBundleIdentifier, TERM (tty path via /dev/tty)
4. Detect agent from parent process name or VIBE_AGENT env var
5. Add session_id from VIBE_SESSION_ID if missing
6. Add cwd if missing
7. Connect to socket at `~/.config/vibe-island/claude.sock` (or VIBE_SOCKET_PATH override)
8. Send JSON + newline
9. For PreToolUse events (has tool_name but no tool_response): wait for response, parse it, if blocked exit with code 2
10. For other events: read ack and exit

- [ ] **Step 3: Test the bridge binary compiles**

Run: `cd src-bridge && cargo build --release`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src-bridge/
git commit -m "feat: add Rust bridge binary to replace hook.py"
```

---

### Task 2: Integrate Bridge Binary Build into Tauri Build

**Files:**
- Modify: `src-tauri/Cargo.toml` — add build.rs to compile bridge
- Create: `src-tauri/build.rs` — compile bridge binary and copy to resources

- [ ] **Step 1: Update build.rs to compile the bridge binary**

Add a build.rs that:
1. Runs `cargo build --release --manifest-path ../../src-bridge/Cargo.toml` (relative to src-tauri)
2. Copies the binary to `~/.config/vibe-island/vibe-bridge` (or a resources dir)

Actually, simpler approach: Use a Makefile or shell script. But even simpler — have the Tauri app's startup code compile/copy the bridge, or use Cargo workspace.

Best approach: Add a workspace Cargo.toml at the project root that includes both src-tauri and src-bridge, and have the Tauri build script copy the bridge binary to the right location.

- [ ] **Step 2: Create workspace Cargo.toml at project root**

```toml
[workspace]
members = ["src-tauri", "src-bridge"]
resolver = "2"
```

- [ ] **Step 3: Update src-tauri/Cargo.toml to reference workspace deps properly**

- [ ] **Step 4: Verify workspace builds**

Run: `cargo build --release -p vibe-bridge`
Expected: builds successfully

- [ ] **Step 5: Commit**

---

### Task 3: Update Hook Registration to Use Bridge Binary

**Files:**
- Modify: `src-tauri/src/claude/mod.rs` — change setup_claude_hooks and setup_gemini_hooks to use vibe-bridge instead of python3

- [ ] **Step 1: Update setup_claude_hooks to use vibe-bridge path**

Change `python3 {script_path}` to `{bridge_path}` where bridge_path is `~/.config/vibe-island/vibe-bridge`.
The bridge binary is copied there on startup if not present.

- [ ] **Step 2: Update setup_gemini_hooks similarly**

Change `VIBE_AGENT=gemini python3 $HOME/.config/vibe-island/hook.py {flag}` to `VIBE_AGENT=gemini $HOME/.config/vibe-island/vibe-bridge {flag}`.

- [ ] **Step 3: Remove hook.py generation code**

Remove the entire `let script = format!(...)` block that generates the Python script. Keep the script_dir creation (for the bridge binary).

- [ ] **Step 4: Add bridge binary deployment on startup**

In start_bridge, after creating the socket, copy the compiled bridge binary from the app bundle's Resources to ~/.config/vibe-island/vibe-bridge. Fall back to searching PATH for vibe-bridge.

- [ ] **Step 5: Run Rust tests**

Run: `cd src-tauri && cargo test`
Expected: all tests pass

- [ ] **Step 6: Commit**

---

### Task 4: Add Terminal Environment Enrichment to Session

**Files:**
- Modify: `src-tauri/src/claude/mod.rs` — add env fields to Session, parse from hook input
- Modify: `src/types/index.ts` — add env fields to Session type
- Modify: `src/hooks/useIPC.ts` — no changes needed (already passes full session)
- Modify: `src-tauri/src/terminal/mod.rs` — use ITERM_SESSION_ID for more precise iTerm2 jumping

- [ ] **Step 1: Add terminal env fields to Session struct in Rust**

Add fields:
```rust
pub term_program: String,          // TERM_PROGRAM
pub iterm_session_id: String,      // ITERM_SESSION_ID
pub tmux_pane: String,             // TMUX_PANE
pub tty: String,                   // /dev/ttysXXX
```

All default to empty string for backward compatibility.

- [ ] **Step 2: Parse env fields from ClaudeHookInput**

Add same fields to ClaudeHookInput, they come from the bridge binary's enrichment.

- [ ] **Step 3: Update Session creation to include env fields**

- [ ] **Step 4: Update TypeScript Session type**

Add corresponding fields to `src/types/index.ts`:
```typescript
termProgram: string;
itermSessionId: string;
tmuxPane: string;
tty: string;
```

- [ ] **Step 5: Update terminal jumping to use iterm_session_id**

In `jump_iterm2_by_pid`, when we have an iterm_session_id, use it directly for exact tab matching instead of PID-based searching.

- [ ] **Step 6: Update all test fixtures to include new fields**

- [ ] **Step 7: Run all tests**

Run: `npm test && cd src-tauri && cargo test`
Expected: all pass

- [ ] **Step 8: Commit**

---

### Task 5: Auto-Repair Hook Configuration on Startup

**Files:**
- Create: `src-tauri/src/claude/hooks.rs` — extracted hook management module
- Modify: `src-tauri/src/claude/mod.rs` — use hooks module

- [ ] **Step 1: Create hooks.rs with verify_and_repair function**

The function should:
1. Read ~/.claude/settings.json
2. Check if hooks section has vibe-island entries for all required events (PreToolUse, PostToolUse, Stop, Notification)
3. If entries are missing or the command path is wrong, fix them
4. Same for ~/.gemini/settings.json
5. Remove stale entries (old python3 hook.py commands)
6. Return a report of what was repaired

- [ ] **Step 2: Call verify_and_repair on startup in start_bridge**

After the initial hook setup, also run verify_and_repair.

- [ ] **Step 3: Add periodic re-verification**

Every 5 minutes, re-verify hooks to handle cases where other tools overwrite settings.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test`
Expected: all pass

- [ ] **Step 5: Commit**

---

### Task 6: OpenCode JavaScript Plugin

**Files:**
- Create: `scripts/opencode-plugin.js` — OpenCode plugin that connects directly to socket

- [ ] **Step 1: Create the OpenCode plugin**

The plugin should:
1. Hook into OpenCode's event system (session events, message events, permission events)
2. Connect to `~/.config/vibe-island/claude.sock` via Node.js `net.connect()`
3. Map OpenCode events to the canonical format
4. For permission/question events: wait for response from socket, reply back to OpenCode

Note: This is a best-effort plugin. OpenCode's plugin API may vary. The plugin should:
- Export a default object with event handlers
- Use `net` module for Unix socket connection
- Send JSON lines and read responses
- Handle reconnection

- [ ] **Step 2: Add installation instructions to hook setup**

In the hook setup code, also install the plugin to `~/.config/opencode/plugins/` and register it in `~/.config/opencode/config.json`.

- [ ] **Step 3: Run all tests**

Run: `npm test && cd src-tauri && cargo test`

- [ ] **Step 4: Commit**

---

### Task 7: Final Integration Testing

- [ ] **Step 1: Run all Rust tests**
- [ ] **Step 2: Run all frontend tests**
- [ ] **Step 3: Build the full app**
- [ ] **Step 4: Update AGENTS.md with new architecture**
- [ ] **Step 5: Final commit**
