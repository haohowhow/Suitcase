# Agent 接手指南

本文件是随 Git 版本管理的接手入口，适用于没有本机 `AGENTS.md` 或聊天历史的 agent。项目状态快照见 [PROJECT_STATUS.md](PROJECT_STATUS.md)。本文不指定下一个产品功能，也不授予提交、发布或真实账户写入权限。

## 开始前先做这些

1. 阅读适用的本地代理指令，再读[项目进度](PROJECT_STATUS.md)、[README](../README.md)、[前端职责](../src/README.md)与[后端职责](../src-tauri/README.md)。遇到状态冲突时以源码与 Git 为准；安全限制不能因实现中的例外而自行放宽。
2. 在仓库根目录运行以下只读命令，记录分支、HEAD、已有改动与脚本定义；不要覆盖用户未完成的工作：

   ```powershell
   git status --short
   git branch -vv
   git log -8 --date=short --format="%h %ad %s"
   Get-Content package.json
   ```

3. 明确本次任务与完成条件。若任务仅为“接手项目”，先报告状态与可选工作，不擅自添加路线图功能。涉及 `crypto/` / `vault_core/` 的实现改动、OAuth 或范围扩展时，先与维护者对齐。
4. 按下表定位负责实现的模块，再阅读[架构说明](ARCHITECTURE.md)的对应小节。版本号以 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 为准；命令注册以 `src-tauri/src/lib.rs` 为准，避免重复实现已有命令。

## 按任务定位代码

| 要处理的事 | 前端入口 | Rust / 配置入口 | 优先检查 |
| --- | --- | --- | --- |
| 登录 / 凭据 / 账户切换 | `src/App.tsx`、`features/auth/` | `commands.rs`、`credential_manager.rs`、`koofr_api/` | 退出时取消任务、清理会话与账户隔离 |
| 文件 / 回收站 / 已连接存储 | `features/files/`、`features/places/` | `commands.rs`、`file_ops/`、`koofr_api/` | Rust 侧路径、mount 与根目录操作校验 |
| 分享链接 | `features/links/`、`services/publicLinks.ts` | `link_commands.rs`、`koofr_api/public_links.rs` | 密码不持久化、创建后配置失败回滚 |
| 普通 / 分卷 / 文件夹传输 | `hooks/useTransferManager.ts`、`features/transfers/` | `transfer/`、`transfer_commands.rs`、`split_commands.rs`、`folder_download.rs` | 续传语义、校验、取消 / 暂停、失败清理 |
| 经典 / 按需同步 | `hooks/useSyncManager.ts`、`features/sync/SyncWorkspace.tsx` | `sync.rs`、`cloud_files.rs`、`commands.rs` | 三方基线、条件删除、范围不重叠、CFAPI 身份 |
| Vault | `features/vault/`、`types/vault.ts` | `vault_commands.rs`、`vault_core/`、`crypto/`、`transfer/vault.rs` | 复用既有实现、Safe Key 边界与互操作测试 |
| 文件预览 | `features/files/FilePreviewModal.tsx`、`preview.ts` | `stream_commands.rs`、`stream_session.rs`、`preview_decode.rs`、`lib.rs` scheme handler | Range、关闭清理、大小上限、Vault 明文内存 |
| 设置 / 桌面 / 工作目录 | `hooks/useAppSettings.ts`、`features/settings/`、`features/transfers/FloatingTransferWindow.tsx` | `settings.rs`、`desktop.rs`、`autostart.rs`、`work_directory.rs` | 启动即生效、托盘退出语义、迁移失败恢复 |
| IPC / 权限 / 安装 / 发布 | `services/koofr.ts`、`types/backend.ts` | `lib.rs`、`capabilities/`、`tauri.conf.json`、`windows/hooks.nsh`；根目录 `scripts/release-forgejo.mjs` | 最小权限、版本一致、标签与产物来源 |

表中前端相对路径除特别说明外均位于 `src/`，Rust 相对路径位于 `src-tauri/src/`；capabilities、Tauri 配置与 Windows hooks 位于 `src-tauri/`。

`App.tsx` 负责跨模块协调，全局设置、传输与同步逻辑已抽取到 `src/hooks/` 中的 hooks，不应重新集中回 App。feature 之间不得新增直接依赖。业务 Tauri 调用仅通过两个 services 文件；当前 `TitleBar.tsx` 直接调用窗口控制属于既有偏差，不构成其他组件绕过 services 的先例。错误处理使用 `commandErrorMessage` / `isCommandErrorCode`。视觉规范遵循 [DESIGN.md](../DESIGN.md) 与 `src/theme.ts`。

## 不得破坏的边界

- **凭据**：登录表单中的应用专用密码提交给 Rust 后即清除；session token 不回传 WebView。仅当用户选择保存时才存入 Windows Credential Manager，当前实现服务名为 `Koofr-GUI`、账号为邮箱，见 `credential_manager.rs`。不得读取或输出真实凭据，不得将密码或 token 写入日志、URL、命令行或 fixture。
- **Vault**：Safe Key 仅通过 Windows 原生提示与 Rust 传递，不经过 IPC 或 React。保留固定的官方 `vault-crypto` 与 rclone 兼容格式，不自行设计加密方案。密文路径与密钥保留在 Rust，前端仅获取显示元数据与短期不透明句柄。导出的 rclone `password` / `password2` 可恢复，须按敏感凭据处理；预览明文仅存在于受限、可清零的 Rust 内存中。
- **本地与远端路径**：Rust 需再次校验 mount、操作范围、远端规范路径、本地绝对路径与符号链接。拒绝针对远端根的破坏性操作。手动上传来自原生选择器授权，下载消费一次性且区分文件 / 文件夹的 auth ID；前端不得决定最终文件名。
- **写入与清理**：手动下载不覆盖，使用专用临时文件 / 目录并稳定避让同名。同步按其三方基线与条件操作协调，不得改为无条件覆盖或删除；不能套用手动下载规则擅自改变同步语义。清理仅处理已验证归属的精确路径；不得扫描或删除用户的应用数据来“修复”测试。
- **传输**：普通与 Vault 上传为整文件重试；分卷按已确认完整的 part 续传，保留裸字节与开放恢复格式；下载才是真正的 Range 字节续传，200 回退需安全截断，远端 validator 变化则失败。自动重试仅适用于 `network_error`，等待期间仍可取消。
- **同步与 CFAPI**：仅支持主存储普通目录，不扩展到 Vault / 共享 / 第三方挂载；拒绝范围重叠。首次合并不传播删除，后续用基线防止误删并保留冲突。占位符身份仅使用不透明 UUID，解除注册或卸载前先核对自己的 Provider 与根身份。完全退出会停止同步，托盘隐藏不会。
- **IPC / 日志 / 代码加载**：错误仅包含 `{ code, message }`，安全消息不含路径或响应正文。诊断日志与持续进度事件不带邮箱、文件名、完整路径、密钥、token 或响应正文；按需任务详情与业务显示数据另走受限命令。不扩大 capabilities，不给 WebView 任意 shell / 文件系统 / 网络能力，不在具有 IPC 的 WebView 中加载远端脚本。

完整行为说明见 [ARCHITECTURE.md](ARCHITECTURE.md)。若要修改协议或加密，先核对官方 Go / Java SDK、Koofr Vault 与 rclone 对应源码；本文件的概述不能替代协议依据。

## 实施与验证

从现有实现出发做最小连贯改动，不顺带升级依赖或重构无关模块。先阅读 `package.json` 中的真实脚本，再执行：

| 情况 | 命令 / 要求 |
| --- | --- |
| 新环境 | 按 [BUILDING.md](BUILDING.md) 配置 Windows MSVC、Node 与 Rust；`npm ci` 复现依赖 |
| 快速循环 | `npm run verify:quick`：lint + SplitUploadDialog 测试 + 前端构建，不代表全测试覆盖 |
| Rust、跨栈或交付前完整验证 | `npm run check`：lint、全部 Vitest（含发布工具）、前端构建、Rust fmt / Clippy / test |
| 故障定位 | `npm run lint`、`npm run test`、`npm run build`、`npm run check:rust`；按失败阶段定向定位 |
| exe / 安装包任务 | 取得对应任务后再使用 `npm run build:desktop` / `npm run build:installer`，并遵守 [RELEASING.md](RELEASING.md) |

`npm run dev` 仅为浏览器前端，无法证明 Tauri 登录、传输、CFAPI 或原生窗口可用。**不得运行 Computer Use、桌面自动化、UI Automation、SendInput 或同等能力来启动、控制、检查或捕获桌面应用。** GUI、资源管理器与安装卸载交由用户手动验收，并报告为“需要人工在本机验证”。

默认 Rust 测试包含 1 项会修改临时 Windows Cloud Files 同步根的 ignored 集成测试；请保持跳过，不擅自添加 `--ignored`。涉及加密修改需保留或补充合成 golden vectors、Unicode / salt / 块边界与篡改拒绝测试，不使用真实 Vault 导出。

文档更新遵守 [CONTRIBUTING.md](../CONTRIBUTING.md)：用户可见的行为改动需同步中英文 README 与 `CHANGELOG.md` 的 Unreleased；架构与命令改动需同步架构 / 模块说明；构建与发布改动需更新对应指南。最后检查 `git diff --check` 与文件清单，确保没有意外修改锁文件、版本或生成物。

## 外部影响与交付

未获用户明确要求时，不执行 `git commit`、`git push`、打 tag、创建 Issue / PR、触发发布或修改真实 Koofr 账户。既有发布标签不得移动；迁移重建的草稿与来源未确认的安装包不视为正式发布。发布脚本 dry run 会校验 HEAD、标签与安装包；失败时应按前提排查，不得绕过校验。

`AGENTS.md`、`CLAUDE.md`、`.agents/`、`.claude/`、`.omo/` 与 `scripts/release-local.ps1` 均为被忽略的本地上下文，不得提交，也不得使用 `git add -f`。跨机器接手应链接本文件与[项目进度](PROJECT_STATUS.md)，不依赖这些本地文件的存在。

交付说明至少包含：行为差异与修改文件、安全影响、实际执行的检查命令及结果、未执行项及原因、需人工验证的内容与遗留问题。不得将历史通过记录视为本轮测试结果，也不得将“已实现”表述为“本机 GUI 验收通过”。

## 交给下一位 agent 的任务模板

复制以下内容并填写具体任务；文档中已包含背景，无需粘贴整段聊天记录：

```text
请在 Koofr-GUI 仓库继续工作，先读适用的本地指令、docs/AGENT_HANDOFF.md
和 docs/PROJECT_STATUS.md，再核对 git status / 分支 / HEAD 与相关源码。

本次具体目标：<一个功能、问题或检查任务；不要留空直接执行>
复现 / 输入：<操作、错误码、样例；不含真实凭据或私密内容>
完成条件：<可观察的行为、必要检查、文档或构建产物>
范围与已有改动：<允许修改的模块、须保留的用户改动>

按接手指南的安全和验收边界完成任务；未明确授权时不提交、推送、发布、
修改真实账户或操控桌面 GUI。交付时分别报告已验证与需人工验证的内容。
```
