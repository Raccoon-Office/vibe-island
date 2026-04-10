# Vibe Island

macOS 桌面浮动窗口，仿苹果灵动岛（Dynamic Island），实时监控多个 AI 编程 Agent 的运行状态。

## 功能

- **实时状态追踪** — 监控 Claude Code、OpenCode、Gemini、Codex 等 AI Agent 的运行/等待/完成状态
- **像素骑士动画** — 每个 Agent 以独特的像素骑士形象呈现，运行时走动/攻击，等待时待机
- **点击跳转** — 点击某个会话，自动跳转到对应的终端标签页
- **自动配置** — 首次启动自动写入 Hook 脚本并注册到 `~/.claude/settings.json` 和 `~/.gemini/settings.json`
- **心跳检测** — 每 15 秒检测 Agent 进程是否存活，自动清理已退出的会话
- **主题切换** — 支持 Default（暗色）和 Forest（绿色）主题
- **透明悬浮窗** — 始终置顶、无边框、透明背景，定位在屏幕右上角

### 支持的 Agent

| Agent | 标识色 | 武器 | 检测方式 |
|-------|--------|------|----------|
| Claude Code | 紫色 | 剑 | Hook 自动注册 |
| OpenCode | 橙色 | 刀 | 通过父进程名自动检测 |
| Gemini | 蓝色 | 法杖 | Hook 自动注册 |
| Codex | 绿色 | 斧头 | 通过父进程名自动检测 |

### 支持的终端

| 终端 | 跳转能力 |
|------|----------|
| iTerm2 | 精确跳转到对应标签页（PID 匹配） |
| Terminal.app | 精确跳转到对应标签页（PID 匹配） |
| Ghostty | 激活窗口 |
| VSCode / Cursor / Windsurf / Zed | 激活窗口 |

## 系统要求

- **macOS**（仅支持 macOS，使用了 cocoa/objc/AppleScript）
- **Node.js** >= 18
- **Rust** 工具链（rustup + cargo）

## 快速开始

### 开发模式

```bash
# 安装前端依赖
npm install

# 启动开发服务器（Vite + Cargo debug 构建）
npm run tauri dev
```

### 生产构建

```bash
npm install
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/macos/Vibe Island.app`。

### 安装到 /Applications

```bash
cp -R src-tauri/target/release/bundle/macos/Vibe\ Island.app /Applications/
```

首次打开若提示"无法验证开发者"，前往 **系统设置 → 隐私与安全性 → 仍要打开**。

## 工作原理

```
┌──────────────────┐     stdin/JSON      ┌──────────────┐    Unix Socket    ┌──────────────────┐
│  Claude Code /   │ ──────────────────→ │ vibe-bridge  │ ────────────────→ │  Vibe Island     │
│  Gemini / etc.   │   (Hook 触发)       │  (Rust 二进制)│   (JSON line)     │  (Rust Backend)  │
└──────────────────┘                     └──────────────┘                   └────────┬─────────┘
                                                                                      │
┌──────────────────┐    direct socket                                        Tauri Event
│  OpenCode        │ ────────────────────────────────────────────────────────────→│
│  (JS Plugin)     │                                                            │
└──────────────────┘                                                             ▼
                                                                       ┌──────────────────┐
                                                                       │  React Frontend  │
                                                                       │  (像素骑士渲染)    │
                                                                       └──────────────────┘
```

1. **Hook 注册** — 启动时自动在 `~/.claude/settings.json` 和 `~/.gemini/settings.json` 中注册 Hook 命令，并每 5 分钟自动修复
2. **事件捕获** — AI Agent 触发 PreToolUse / PostToolUse / Stop / Notification 时，Bridge 二进制注入终端环境变量并通过 Unix Domain Socket 发送 JSON 到后端
3. **状态管理** — Rust 后端维护会话列表，通过 Tauri 事件推送到前端
4. **可视化** — React 前端用 CSS 像素艺术渲染每个 Agent 的状态动画

## 自动创建的文件

首次启动后，以下文件会被自动创建/修改：

| 文件 | 说明 |
|------|------|
| `~/.config/vibe-island/claude.sock` | Unix Domain Socket（通信通道） |
| `~/.config/vibe-island/vibe-bridge` | Rust Bridge 二进制（替代 Python hook） |
| `~/.claude/settings.json` | 注册 PreToolUse / PostToolUse / Stop / Notification Hook |
| `~/.gemini/settings.json` | 注册 SessionStart / BeforeTool / AfterTool / AfterAgent / SessionEnd Hook |
| `~/.config/opencode/plugins/vibe-island.js` | OpenCode JS 插件（直接 socket 通信） |
| `~/Library/Local/VibeIsland/logs/` | 日志目录 |

无需手动配置任何文件。

## 项目结构

```
├── index.html                    # Vite 入口 HTML
├── package.json                  # 前端依赖
├── vite.config.ts                # Vite 配置（端口 1420）
├── src/                          # 前端（React + TypeScript）
│   ├── main.tsx                  # 入口
│   ├── App.tsx                   # 主界面 — 窗口控件、主题切换、会话列表
│   ├── styles.css                # 像素骑士 CSS 动画、Dynamic Island 样式
│   ├── hooks/useIPC.ts           # Tauri 事件监听、会话状态管理
│   └── types/index.ts            # TypeScript 类型定义
├── src-tauri/                    # 后端（Rust + Tauri v2）
│   ├── Cargo.toml                # Rust 依赖
│   ├── build.rs                  # 构建脚本（编译并复制 bridge 二进制）
│   ├── tauri.conf.json           # Tauri 窗口/构建配置
│   ├── capabilities/default.json # Tauri v2 权限
│   └── src/
│       ├── main.rs               # 二进制入口
│       ├── lib.rs                # 核心初始化 — 窗口透明化、位置定位、状态管理
│       ├── claude/mod.rs         # Hook 桥接 — Socket 服务器、会话管理、Hook 注册/自动修复
│       ├── ipc/mod.rs            # IPC 事件类型
│       └── terminal/mod.rs       # 终端跳转 — AppleScript 各终端适配（支持 ITERM_SESSION_ID）
└── src-bridge/                   # Bridge 二进制（Rust，替代 Python hook.py）
    ├── Cargo.toml
    └── src/main.rs               # stdin → 注入环境变量 → socket → 等待响应
```

## 技术栈

- **前端**: React 18 + TypeScript + Vite
- **后端**: Rust + Tauri v2 + Tokio
- **原生集成**: cocoa / objc（窗口透明化）+ AppleScript（终端跳转）
- **IPC**: Unix Domain Socket（Rust Bridge ↔ Rust 后端）+ Tauri Events（Rust ↔ React）

## 分发

将构建好的 `.app` 压缩后发给对方即可：

```bash
cd src-tauri/target/release/bundle/macos
zip -r ~/Desktop/VibeIsland.zip "Vibe Island.app"
```

对方解压后拖入 `/Applications` 目录使用。无需额外依赖。
