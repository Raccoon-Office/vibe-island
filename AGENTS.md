# AGENTS.md

## 项目概述

Vibe Island — macOS 桌面浮动窗口，仿苹果灵动岛，实时监控 AI 编程 Agent 状态。

## 技术栈

- **前端**: React 18 + TypeScript + Vite
- **后端**: Rust + Tauri v2 + Tokio
- **测试**: Vitest + @testing-library/react
- **原生集成**: cocoa/objc + AppleScript

## 常用命令

| 命令 | 说明 |
|------|------|
| `npm install` | 安装前端依赖 |
| `npm run tauri dev` | 开发模式（Vite + Cargo debug） |
| `npm run tauri build` | 生产构建 |
| `npm test` | 运行测试 |
| `npm run build` | 仅前端构建（tsc + vite build） |

## 开发规范

- **每次改动后必须运行测试**: `npm test`
- **发布流程**: 提交代码 → `npm run tauri build` 打包 → 产物在 `src-tauri/target/release/bundle/macos/`
- **不要跳过测试步骤**，即使看起来是小改动
- **不要自动 commit**，除非用户明确要求

## 环境依赖

- macOS only
- Node.js >= 18
- Rust 工具链（rustup + cargo）— 确保 `~/.zshrc` 中有 `export PATH="$HOME/.cargo/bin:$PATH"`

## 项目结构

- `src/` — React 前端（组件、hooks、类型、样式、测试）
- `src-tauri/` — Rust 后端（Tauri app，socket 服务器、IPC、终端跳转、Hook 注册）
- `src-bridge/` — Rust bridge 二进制（替代 hook.py，无需 Python）
- `src/test/` — 测试工具和 setup

## 架构

- **Bridge 二进制** (`vibe-bridge`): 编译的 Rust 小程序，替代 Python hook 脚本。读取 stdin JSON，注入终端环境变量，发送到 Unix socket
- **Hook 自动修复**: 每次启动和每 5 分钟自动校验并修复 Claude/Gemini 的 Hook 配置
- **终端环境注入**: 自动采集 TERM_PROGRAM、ITERM_SESSION_ID、TMUX_PANE、TTY 等环境变量
- **OpenCode 插件**: 自动安装 JS 插件到 `~/.config/opencode/plugins/`，直接 socket 通信
