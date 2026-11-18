<div align="center">

# Koofr-GUI

**Windows 优先的 Koofr 桌面文件管理客户端**

基于 Tauri v2、React、TypeScript 与 Rust 构建，目标是提供贴近 Windows 原生文件管理器的操作体验，而非网页界面的简单封装。

[![Source: Forgejo](https://img.shields.io/badge/Source-Forgejo-fb923c)](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6?logo=windows)](#系统要求)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri)](https://v2.tauri.app/)

[English](README.en.md) · [下载安装包](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases) · [构建指南](docs/BUILDING.md) · [架构说明](docs/ARCHITECTURE.md) · [更新日志](CHANGELOG.md)

恢复开发或交接：先读 [项目进度与恢复工作](docs/PROJECT_STATUS.md)，再读 [Agent 接手指南](docs/AGENT_HANDOFF.md)。

</div>

---

## 目录

- [Koofr-GUI](#koofr-gui)
  - [目录](#目录)
  - [简介](#简介)
  - [截图](#截图)
  - [功能](#功能)
    - [已实现的功能](#已实现的功能)
    - [尚未实现的功能](#尚未实现的功能)
  - [下载与安装](#下载与安装)
  - [系统要求](#系统要求)
    - [运行](#运行)
    - [从源码构建](#从源码构建)
  - [从源码构建](#从源码构建-1)
  - [常用命令](#常用命令)
  - [架构与安全边界](#架构与安全边界)
  - [数据存储](#数据存储)
  - [路线图](#路线图)
  - [参与贡献](#参与贡献)
  - [报告安全问题](#报告安全问题)
  - [许可证](#许可证)
  - [致谢](#致谢)

## 简介

[Koofr](https://koofr.eu) 是一家欧洲的云存储服务商，支持把 Google Drive、OneDrive、Dropbox 等第三方存储聚合到同一个账户。

Koofr-GUI 是一个第三方、Windows 优先、面向原生文件管理器体验的替代客户端，与 Koofr 官方无关联。Koofr 官方已提供桌面客户端，本项目则提供另一种更贴近 Windows 原生文件管理器操作习惯的选择。当前版本已实现 Koofr 文件管理、可续传传输、分享链接管理、常用文件本地预览，以及与 Koofr Vault / rclone crypt 互操作的端到端加密工作区。

## 截图

> 主界面截图待补充。当前设计目标为 1280×720，最小窗口 980×640，遵循 [DESIGN.md](DESIGN.md) 定义的设计系统。

## 功能

### 已实现的功能

- **登录与凭据**
  - 使用 Koofr 应用专用密码登录，下次启动自动恢复会话，并支持主动退出登录。
  - 应用专用密码可选择保存到 Windows 凭据管理器，不会写入普通配置文件或 WebView 存储。

- **文件浏览与操作**
  - 浏览挂载点、目录、最近文件、共享内容和回收站。
  - 识别账户中已连接的 Koofr、Google Drive、OneDrive、Dropbox 等存储。
  - 支持新建文件夹、上传、下载、重命名、移动、复制、删除及从回收站恢复。
  - 分享链接：查询、创建、撤销下载链接与接收文件链接，撤销操作需要二次确认。

- **传输**
  - 传输队列面板提供实时进度，支持取消与暂停。
  - 分卷上传在列表与详情中显示数据分卷、整文件校验进度与预计剩余时间，以及辅助文件的处理进度。
  - 清单上传中断后可从传输面板继续，复用已保存的整文件校验值，并补传缺少的校验文件、清单和还原脚本。
  - 可选置顶传输悬浮窗，聚合显示并发上传 / 下载百分比，暂停或空闲时收为单图标方块。自动收起、单击批量暂停 / 继续与双击打开应用可分别关闭；整个表面仍可用左键拖动，右键菜单可打开应用、关闭悬浮窗或退出应用。该窗口只接收脱敏汇总状态。
  - **单文件下载**：基于 HTTP Range 与磁盘检查点实现字节级续传。
  - **分卷上传**：将大文件切分为用户可配置大小的 `part-*.bin`，中断后从最后一个已确认完整的分卷继续传输。远端表现为一个由用户命名的普通文件夹，可直接使用 Windows `copy /b` 或 POSIX `cat` 还原原文件。
  - **递归文件夹下载**：先暂存到临时目录，失败或取消时清理整个暂存树。

- **本地文件夹同步（Windows 10/11）**
  - 支持多组“本地文件夹 ↔ Koofr 主存储普通文件夹”双向同步；客户端运行或最小化到托盘时保持同步，完全退出后停止。
  - 可选“按需同步（预览）”：通过 Windows Cloud Files API 在资源管理器中显示原生状态，目录展开时创建占位符，打开在线文件时才下载，并支持系统“始终保留在此设备”与“释放空间”。
  - 按需同步支持统一的 `%USERPROFILE%\Koofr` 根（映射 `/`）与自定义映射；同一账户的本地与远端范围不得重叠，暂不支持 Vault、共享空间或第三方挂载。
  - 采用本地监听加 60 秒远端扫描：首轮仅合并、不传播删除；后续依据三方基线传播单侧修改 / 删除，并为双侧修改保留冲突副本。
  - 每组可立即同步、暂停 / 继续、查看文件级问题或移除配置。经典同步的移除仅删除索引；按需根解除注册会将已水合文件保留为普通文件、移除纯在线占位符，云端数据不变。根目录 `.syncignore` 支持注释、glob 与 `!` 反向包含。
  - 同步状态与索引写入工作目录的 `sync-state.sqlite3`，按账户隔离，不进入手动传输恢复列表或悬浮传输窗。

- **本地文件预览**
  - 可在“我的文件”、最近文件、已共享和 Vault 中预览 PDF、纯文本 / 代码、常用及 RAW 图片、视频与音频；回收站不提供预览。双击受支持格式会打开预览，不支持的格式仍直接下载。
  - PDF 按 Range 分页读取；文本最多显示前 5 MiB，支持 UTF-8、UTF-16、GB18030 与 Big5；SVG 会经净化处理，TIFF / HEIC / RAW 均由离线打包的本地解码器处理。Office、OpenDocument、PSD、AI 与电子书仍需下载后打开。
  - Rust 侧通过自定义 `koofr-stream` URI scheme 代理 Range 请求，Koofr 令牌不会暴露给 WebView。普通文件完整解码上限为 100 MiB；Vault 明文仅驻留 Rust 内存且上限 50 MiB，关闭、锁定、切换账户或会话过期时立即清零。

- **Koofr Vault（端到端加密）**
  - 支持创建、导入、导出、解锁、锁定与移除 Vault 注册，并可解密浏览、新建文件夹、重命名、移动、复制和删除。
  - Safe Key 仅在 Windows 原生凭据窗口中输入并保留在 Rust 内存；WebView 只接收解密后的显示元数据与短期不透明句柄。
  - 文件名使用 AES-EME，内容使用 rclone crypt 的 `RCLONE\0\0` + XSalsa20-Poly1305 分块格式，密钥由兼容 rclone 默认参数的 scrypt 派生。
  - 加密上传支持整文件安全重试；加密下载先按密文字节 Range 续传，完成认证与解密后再原子写入最终明文文件。
  - 支持导入 / 导出 rclone crypt 配置。导出的配置包含可恢复 Safe Key 的混淆值，须按敏感凭据保管。

- **设置**
  - 应用启动即加载全部设置；界面语言可在简体中文、英文与跟随系统之间即时切换，紧凑密度与减少动态效果立即生效。
  - 支持开机启动、关闭 / 最小化到系统托盘、退出传输确认、原生完成通知与任务栏活动数角标。应用以单实例运行，再次启动会唤起已有窗口；开机自启且启用关闭到托盘时仅驻留托盘。
  - 退出应用、退出登录（含空闲自动登出）或切换账户时，进行中的传输会暂停而非取消；可续传的任务在再次登录后可继续。
  - 首次使用时询问是否启用传输悬浮窗，之后可随时在通知设置中开关；拖动位置会在多显示器范围内恢复。
  - 上传 / 下载并发数与全局 / 单任务 KiB/s 限速实时生效；“系统空闲时全速”以 Windows 连续 5 分钟无键盘鼠标输入为准。
  - 下载冲突可设为询问、跳过或稳定加序号，始终不覆盖现有文件；可启用 Koofr MD5 校验、失败分片保留期限与完成后打开文件夹。
  - 分享链接可设置默认类型、有效期与服务端随机密码；密码仅在创建结果中显示一次，不写入本地数据。
  - 下载历史按账户应用保留天数与条数限制，`0` 表示永久保留 / 不限。
  - 默认下载文件夹可配置，每次下载可选择“询问位置”。
  - 可选择在 `network_error` 后自动恢复传输；最大重试次数与固定重试间隔均可设置，次数可设为无限，等待期间仍可暂停或取消。连接停滞（下载 60 秒无数据、上传 5 分钟无进度）同样按网络错误处理。
  - 应用工作目录可自定义；切换时可选择在下次启动前全量迁移当前设置、传输状态、下载历史、缓存与日志。
  - 元数据缓存可选内存 / 磁盘 / 关闭；诊断日志的级别、保留时间与单文件大小可配置，并支持查看占用与一键清理。
  - 排查停滞时，可在「设置 → 诊断日志」选择「调试信息」，查看请求耗时、分卷校验进度和收尾、同步或预览当前所处的阶段。
  - 提供 5 套主题（koofr / ocean / iris / coral / berry），仅覆盖 accent 色。

### 尚未实现的功能

- OAuth 登录，以及第三方存储的新增、移除与重新授权。当前登录仅支持 Koofr 应用专用密码；第三方挂载管理需在 Koofr 官方网页端完成。
- macOS / Linux 支持。

> **普通上传为什么不能续传？** Koofr 公开上传接口仅提供整文件 `FilesPut`，不支持分块会话或服务端偏移确认，因此普通上传中断后必须重新传输整个文件。分卷上传是针对该限制选定的互操作方案，详见 [ARCHITECTURE.md](docs/ARCHITECTURE.md#分卷上传设计)。

## 下载与安装

已发布的安装包（如有）会列在 Releases 页面。建议从 [Releases 页面](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases) 下载最新的 NSIS 安装包（例如 `Koofr GUI_<version>_x64-setup.exe`）。

> ⚠️ **未签名警告**：安装包当前不做代码签名，Windows SmartScreen 可能提示“未知发布者”。请仅从本仓库的 Releases 页面下载，不要使用第三方镜像。校验方式详见 [RELEASING.md](docs/RELEASING.md)。

首次运行需要 Microsoft Edge WebView2 Runtime。受支持的 Windows 10 / 11 通常已自带；若缺失，安装包会引导下载。

## 系统要求

### 运行

- Windows 10 或 11（x64）
- Microsoft Edge WebView2 Runtime（通常自带）
- Koofr 账户 + [应用专用密码](https://app.koofr.net/app-password)

### 从源码构建

- Node.js **24 LTS**（推荐；兼容 22.12 及以上的 22.x LTS）
- npm **10+**
- Rust **1.88+**，`x86_64-pc-windows-msvc` 工具链
- Visual Studio 2022 Build Tools 的“使用 C++ 的桌面开发”工作负载及 Windows SDK

完整安装步骤见 [BUILDING.md](docs/BUILDING.md#1-环境要求)。

## 从源码构建

```powershell
git clone https://forgejo.haohaohow.com/sora-admin/Koofr-GUI.git
Set-Location Koofr-GUI
npm ci
npm run dev:desktop          # 开发模式：完整桌面应用
```

仅启动前端开发服务器（在浏览器中查看，无法测试 Tauri 命令）：

```powershell
npm run dev
```

发布构建：

```powershell
npm run check                # 完整质量检查：lint + 测试 + Rust fmt/clippy
npm run build:desktop        # 生成 src-tauri/target/release/koofr-gui.exe
npm run build:installer      # 显式生成 NSIS 安装包
```

详细说明与常见问题见 [BUILDING.md](docs/BUILDING.md)。

## 常用命令

| 命令 | 用途 |
| --- | --- |
| `npm ci` | 按 `package-lock.json` 安装依赖，保证可复现 |
| `npm run dev` | 启动 Vite 前端开发服务器（仅浏览器） |
| `npm run dev:desktop` | 启动完整的 Tauri 桌面开发环境 |
| `npm run build` | 类型检查并生成 `dist/` 前端资源 |
| `npm run build:desktop` | 构建 Windows 发布版 exe（不生成安装包） |
| `npm run build:installer` | 显式构建 NSIS 安装包（发布流程使用） |
| `npm run verify:quick` | 快速验证：lint + 拆分上传测试 + 前端构建 |
| `npm run check` | 完整验证：lint + 测试 + 前端构建 + Rust fmt + clippy + Rust 测试 |
| `npm run clean` | 清理构建产物，保留 `node_modules/` |
| `npm run clean:all` | 清理构建产物与 `node_modules/`（之后需重新执行 `npm ci`） |

## 架构与安全边界

```
┌──────────────────────────────────────────────┐
│ React + TypeScript UI (src/)                 │
│ - 只调用受限的 Tauri 命令                    │
│ - 不接触密码、令牌、Vault Safe Key           │
└──────────────┬───────────────────────────────┘
               │ typed Tauri commands + events
               ▼
┌──────────────────────────────────────────────┐
│ Rust + Tauri core (src-tauri/src/)           │
│ ├─ file_ops/           路径与操作校验         │
│ ├─ transfer/           上传/下载/续传/进度    │
│ ├─ koofr_api/          Koofr REST 客户端      │
│ ├─ credential_manager  Windows 凭据管理器     │
│ ├─ metadata_cache      内存/磁盘缓存          │
│ ├─ crypto/             rclone crypt 兼容边界   │
│ └─ vault_core/         Vault 会话与不透明句柄  │
└──────────────────────────────────────────────┘
```

关键规则：

- **凭据不越出 Rust 边界**：会话令牌仅存于内存；应用专用密码可选择存入 Windows 凭据管理器。
- **Safe Key 不越出 Rust 边界**：由 Windows 原生凭据窗口直接交给 Rust，不经过 Tauri IPC、React 状态、日志或磁盘检查点。
- **路径必须规范**：远程路径拒绝 `.`、`..`、NUL 及超长段；本地下载目录必须为绝对路径、已存在且不是符号链接。
- **不覆盖现有文件**：下载先写入 `.koofr-part-*` 临时文件；选择名称时避开已有文件与其他未完成下载的目标，发布到最终位置时也不会替换已有文件，名称被占用时自动追加序号。
- **错误消息不泄露路径、令牌或响应正文**，仅返回稳定错误码与安全消息。

完整设计与数据流见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 数据存储

应用数据默认保存在 Windows 用户目录下（`identifier = net.koofr.desktop.gui`），也可在设置中改为已有的空目录：

```
<应用工作目录>\
├─ settings.json                # 应用设置
├─ transfer-checkpoints.json    # 传输恢复检查点
├─ download-history.json        # 按账户隔离的下载历史、文件位置与限量速度采样
├─ sync-state.sqlite3           # 按账户隔离的同步组、基线、问题与操作日志（WAL）
├─ cache/metadata-cache.json    # 磁盘元数据缓存
└─ logs/koofr-gui*.jsonl        # 脱敏诊断日志
```

默认工作目录为 `%LOCALAPPDATA%\net.koofr.desktop.gui`。其旁边另有一个体积很小的
`net.koofr.desktop.gui.work-directory.json` 定位文件，用于应用重启后定位自定义
工作目录；全量迁移不会迁移该定位文件。迁移在下次启动时、任何工作文件被打开前执行，
支持跨磁盘复制，并在成功切换后清理旧目录；目标目录必须为空，失败时保留可用副本并在下次启动时重试。

**凭据不保存在工作目录中**：Koofr 应用密码通过 Windows 凭据管理器保存，重装或
重构建应用后仍然存在，也不会随工作目录迁移。

## 路线图

- [x] Koofr Vault 解锁、加解密与 Vault 传输（兼容 rclone crypt）
- [ ] OAuth 登录与第三方存储管理（等待 Koofr 公开桌面客户端 API）
- [ ] 代码签名 / SmartScreen 信誉
- [ ] macOS 支持
- [ ] Linux 支持

## 参与贡献

欢迎提交 Issue 与 Pull Request。提交前请：

1. 阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。
2. 执行 `npm run check`，确保 lint、测试、Rust fmt / clippy 全部通过。
3. 新增 Tauri 命令时，Rust 侧必须校验路径、标识符与操作范围。

## 报告安全问题

**请勿**通过公开 Issue 上报漏洞，请使用 [SECURITY.md](SECURITY.md) 中的私密渠道。

## 许可证

[MIT](LICENSE) © 2026 Koofr-GUI 贡献者

本项目与 Koofr d.o.o. 无关联。“Koofr”是 [Koofr d.o.o.](https://koofr.eu) 的商标。

## 致谢

- [Koofr](https://koofr.eu) 提供的云存储服务，以及 [Go 客户端](https://github.com/koofr/go-koofrclient) / [Java SDK](https://github.com/koofr/java-koofr) 参考实现。
- [Tauri](https://tauri.app) 团队提供的桌面框架。
- 加密库使用 [Koofr Vault](https://github.com/koofr/vault) 提供的 `vault-crypto`，并与 [rclone](https://rclone.org) 的 crypt 格式保持互操作。
- [Lucide](https://lucide.dev) 提供的图标集。
