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
- Python 3（Hook 脚本需要）

## 项目结构

- `src/` — React 前端（组件、hooks、类型、样式、测试）
- `src-tauri/src/` — Rust 后端（socket 服务器、IPC、终端跳转）
- `src/test/` — 测试工具和 setup
