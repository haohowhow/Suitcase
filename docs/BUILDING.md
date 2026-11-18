# 构建指南

Windows 上从源码构建 Koofr-GUI 的完整流程。所有命令均在仓库根目录执行，示例使用 PowerShell。

## 目录

- [1. 环境要求](#1-环境要求)
- [2. 获取代码与安装依赖](#2-获取代码与安装依赖)
- [3. 开发运行](#3-开发运行)
- [4. 构建](#4-构建)
- [5. 质量检查](#5-质量检查)
- [6. 清理构建产物](#6-清理构建产物)
- [7. 常见问题](#7-常见问题)

## 1. 环境要求

Koofr-GUI 基于 Tauri v2、React、TypeScript 与 Rust，需要以下组件：

| 组件 | 版本 | 说明 |
| --- | --- | --- |
| 操作系统 | Windows 10 / 11（x64） | 目前仅支持 Windows |
| Node.js | 24 LTS（推荐）；兼容 22.12 及以上的 22.x LTS | 从[官方下载页](https://nodejs.org/en/download)获取 LTS 版本 |
| npm | 10+ | 随 Node.js 一同安装 |
| Rust | 1.88+，`x86_64-pc-windows-msvc` 工具链 | 使用 [rustup](https://rustup.rs/) 安装 |
| Visual Studio 2022 Build Tools | 含“使用 C++ 的桌面开发”工作负载及 Windows SDK | Tauri 编译原生代码所需 |
| Microsoft Edge WebView2 Runtime | 受支持的 Windows 通常已自带 | 运行时依赖 |

完整安装方式以 [Tauri Windows 前置要求](https://v2.tauri.app/start/prerequisites/)为准。

文件预览会将 `src-tauri/third_party/libraw-0.21.1/` 中固定的官方 LibRaw
源码静态编译进 Rust 后端，因此 Visual Studio 的 C++ 编译器为必需项，首次
Rust 构建还需额外承担一段原生编译时间。PDF.js worker、CMaps、标准字体、
CSP 安全的图像解码回退与 HEIC WASM 均从锁定的 npm 依赖离线打包，构建及
安装包运行时不从 CDN 获取代码。
第三方版本与许可证见 [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md)。

安装完成后重启 PowerShell 并检查：

```powershell
node --version
npm --version
rustc --version
cargo --version
rustup show active-toolchain
```

最后一条应显示 MSVC 工具链，例如 `stable-x86_64-pc-windows-msvc`。若不是：

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

## 2. 获取代码与安装依赖

```powershell
git clone https://forgejo.haohaohow.com/sora-admin/Koofr-GUI.git
Set-Location Koofr-GUI
npm ci
```

请使用 `npm ci` 而非 `npm install`：前者严格按照已提交的 `package-lock.json` 安装前端与 Tauri CLI 依赖，不会修改锁文件。Rust 依赖在首次开发、检查或构建时依据 `src-tauri/Cargo.lock` 自动下载。

**请勿提交** `node_modules/`、`dist/`、`src-tauri/gen/` 或 `src-tauri/target/`，这些目录均可重新生成。

## 3. 开发运行

### 仅运行前端（浏览器）

```powershell
npm run dev
```

开发服务器固定监听 `http://127.0.0.1:1420/`。该模式**无法**验证依赖 Tauri 命令的桌面功能，登录、传输与文件操作均不可用，仅适合调整 UI 布局或样式。

### 运行完整桌面应用

```powershell
npm run dev:desktop
```

同时启动 Vite 开发服务器与 Tauri 调试进程。首次编译 Rust 耗时较长（数分钟），后续增量编译会明显加快。

### 仅前端热重载，不重启 Rust

`npm run dev:desktop` 启动后，前端代码改动通过 Vite HMR 生效，Rust 端**不会重启**。适合在调试 UI 的同时避免中断正在运行的传输。

### 使用 VS Code 运行、调试与构建

用 VS Code 打开仓库根目录，进入“运行和调试”（`Ctrl+Shift+D`），在下拉框选择入口后点击绿色运行按钮：

| 入口 | 行为 |
| --- | --- |
| 桌面：开发运行（热重载） | 执行 `npm run dev:desktop`，启动完整桌面应用并保留前端 HMR；该入口不提供 Rust 断点调试 |
| 桌面：Rust 断点调试 | 先执行 `npm run tauri -- build --debug --no-bundle`，再通过 Microsoft C/C++ 调试器启动 Debug EXE，可在 Rust 源码设置断点 |
| 前端：Edge 断点调试（仅 UI） | 自动启动 Vite 并打开 Edge，可在 TypeScript / TSX 源码设置断点；不提供 Tauri 桌面命令 |
| 构建：Windows EXE（Release） | 执行 `npm run build:desktop` |
| 构建：NSIS 安装包 | 执行 `npm run build:installer` |
| 构建：前端 | 执行 `npm run build` |

配置见 [launch.json](../.vscode/launch.json) 与 [tasks.json](../.vscode/tasks.json)。`F5` 启动所选入口；`Ctrl+Shift+B` 默认构建 Release 桌面 EXE，也可通过“终端 → 运行任务”选择 Debug 构建、安装包、前端构建或快速检查。

Rust 断点调试需要 Microsoft C/C++ 扩展（`ms-vscode.cpptools`），已列入[扩展推荐](../.vscode/extensions.json)。该入口使用打包后的前端资源，无需 Vite，也不提供 HMR；前端改动后需重新启动调试以重新构建资源。在 WebView 内调试前端可使用桌面开发模式的开发者工具。

启动桌面入口前请完全退出已有的 Koofr 实例（包括托盘中的进程），以免单实例机制将操作转交给旧进程。开发运行与 Edge 调试共用端口 1420，请勿同时启动。Edge 调试结束后 Vite 会保留运行以便再次调试；结束开发时通过“终端 → 终止任务”停止 `Koofr: Vite 开发服务`，桌面开发模式也可在对应终端按 `Ctrl+C` 停止。

上述配置不改变应用权限、账户或发布流程；桌面启动、断点命中与停止行为需人工在本机验证。

## 4. 构建

### 4.1 前端静态资源

```powershell
npm run build
```

依次执行 TypeScript 项目构建与 Vite 生产构建，输出到 `dist/`，不生成 Windows 桌面程序。

### 4.2 Windows 桌面程序

建议先做快速验证再构建：

```powershell
npm run verify:quick    # lint + 分卷上传测试 + 前端构建
npm run build:desktop
```

`npm run build:desktop` 通过 Tauri 自动触发前端生产构建，随后编译 Rust 发布版本，并以 `--no-bundle` 仅生成 exe。成功后产物位于：

```text
src-tauri/target/release/koofr-gui.exe
```

### 4.3 NSIS 安装包

```powershell
npm run build:installer
```

生成 NSIS 安装包到：

```text
src-tauri/target/release/bundle/nsis/Koofr GUI_<version>_x64-setup.exe
```

`tauri.conf.json` 中保留了 NSIS 配置。本地 `build:desktop` 不生成安装包，以便加快迭代。当前流程为在 Windows 本机构建并上传 Forgejo Release 草稿，详见 [RELEASING.md](RELEASING.md)。

> ⚠️ **发布包不做代码签名**。Windows 可能提示未知发布者或触发 SmartScreen 警告，原因与权衡见 [SECURITY.md](../SECURITY.md#已知的非漏洞行为)。

## 5. 质量检查

### 完整检查（提交 PR 前执行）

```powershell
npm run check
```

按顺序执行：

1. **ESLint**（`npm run lint`）
2. **Vitest**（`npm run test`）
3. **TypeScript 与 Vite 生产构建**（`npm run build`）
4. **`cargo fmt --check`**
5. **Clippy**，警告视为错误（`-D warnings`）
6. **Rust 单元测试**（`cargo test`）

任一步骤失败，整个流程立即终止。

### 单独执行

```powershell
npm run lint            # 仅执行 ESLint
npm run test            # 仅执行前端测试
npm run build           # 仅执行前端类型检查与构建
npm run check:rust      # 仅执行 Rust 三项检查（fmt + clippy + test）
```

### 快速验证

```powershell
npm run verify:quick    # lint + 分卷上传测试 + 前端构建
```

耗时明显少于 `npm run check`，适合小改动自检；正式提交前仍建议执行完整的 `npm run check`。

## 6. 清理构建产物

清理命令仅删除仓库内固定、可重新生成的目录。执行前**必须退出**开发服务器与正在运行的 `koofr-gui.exe`，否则 Windows 可能因文件占用而无法删除 Rust 产物。

如需预览清理范围而不实际删除：

```powershell
node scripts/clean.mjs build --dry-run
```

### 6.1 常规清理

```powershell
npm run clean
```

删除以下目录：

- `dist/`：Vite 生产构建产物
- `node_modules/.tmp/`：TypeScript 增量信息
- `node_modules/.vite/`：Vite 依赖缓存
- `src-tauri/target/`：Rust 调试、发布与增量编译产物
- `src-tauri/gen/`：Tauri 自动生成的 schema

`node_modules/` 中的依赖会保留，之后可直接重新构建。

### 6.2 分层清理

仅清理前端：

```powershell
npm run clean:frontend
```

仅清理 Rust / Tauri：

```powershell
npm run clean:rust
```

### 6.3 完全重置

```powershell
npm run clean:all
npm ci
```

`clean:all` 在常规清理基础上删除整个 `node_modules/`，适用于依赖损坏、切换 Node.js 主版本或释放磁盘空间。**重新开发或构建前必须再次执行 `npm ci`**。

### 6.4 清理脚本不会触及的内容

- 源码
- `package-lock.json`、`src-tauri/Cargo.lock`
- Git 内部文件
- Windows 凭据管理器中保存的 Koofr 凭据
- `%LOCALAPPDATA%\net.koofr.desktop.gui\` 中的应用设置与缓存
- 用户下载目录中的 `.koofr-part-*` 临时文件

如需清理上述内容，请手动操作，或使用应用内的“清除缓存 / 忘记登录”功能。

## 7. 常见问题

### 找不到 `link.exe`、Windows SDK 或 C++ 工具

打开 Visual Studio Installer，为 Visual Studio Build Tools 添加“使用 C++ 的桌面开发”工作负载与 Windows SDK，完成后重启终端。

### Rust 使用了 GNU 工具链

本项目在 Windows 上需要 MSVC 工具链：

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

### 端口 1420 被占用

查找并终止占用 `127.0.0.1:1420` 的进程。Vite 配置启用了 `strictPort`，不会自动切换端口（Tauri 开发配置依赖固定地址）。

```powershell
# 查找占用进程
netstat -ano | Select-String ":1420"
# 终止进程（PID 取自上一步输出）
taskkill /PID <PID> /F
```

### 清理时出现 `EPERM` 或“文件正在使用”

请关闭 Vite、Tauri、`koofr-gui.exe` 以及可能正在扫描构建目录的工具（IDE 索引、杀毒软件），然后重新执行清理。请勿手动删除不熟悉的系统目录；项目脚本只操作仓库内的白名单路径。

### 首次编译 Rust 耗时较长

属于正常现象。Tauri v2 依赖链较大，首次全量编译在中等配置的笔记本上可能需要 3–8 分钟，之后使用增量编译会明显加快。若长时间停留在某个 crate 无进展，请检查 `%CARGO_HOME%\.cargo\config.toml` 是否配置了错误的镜像。

### `error: Microsoft Visual C++ 14.0 or greater is required`

Windows SDK 版本过旧或未安装。请安装 Visual Studio 2022 Build Tools 的 Windows 11 SDK（10.0.22621.0 或更新版本）。

### `webview2` 报错找不到 Runtime

目标机器未安装 WebView2 Runtime。开发机通常已自带；若确实缺失，请从 [Microsoft 官方](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) 安装 Evergreen Bootstrapper。

### 确认干净构建

```powershell
npm run clean:all
npm ci
npm run check
npm run build:desktop
```

该流程从锁文件重新安装 JavaScript 依赖，并重新生成全部前端与 Rust 构建产物，建议在发布前执行一次。
