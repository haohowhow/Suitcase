# Agent 接手指南

本文件是随 Git 版本管理的接手入口，适用于没有本机 `AGENTS.md` / 聊天历史的 agent。项目状态快照见 [PROJECT_STATUS.md](PROJECT_STATUS.md)。本文不指定下一个产品功能，也不授予提交、发布或真实账户写入权限。

## 开始前先做这些

1. 阅读适用的本地代理指令，再读 [项目进度](PROJECT_STATUS.md)、[README](../README.md)、[前端职责](../src/README.md)、[后端职责](../src-tauri/README.md)。遇到状态冲突，核对源码与 Git；安全限制不能因实现例外自行放宽。
2. 在仓库根运行下面的只读命令，记录分支、HEAD、已有改动与脚本定义；不要覆盖用户未完成的内容：

   ```powershell
   git status --short
   git branch -vv
   git log -8 --date=short --format="%h %ad %s"
   Get-Content package.json
   ```

3. 明确本次任务和完成条件。若只有“接手项目”，先报告状态和可选工作；不擅自添加路线图功能。涉及 `crypto/` / `vault_core/` 的实现改动、OAuth 或范围扩展，先与维护者对齐。
4. 按下表找负责实现，再读 [架构说明](ARCHITECTURE.md) 的对应小节。版本号以 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 为准；命令注册以 `src-tauri/src/lib.rs` 为准，避免重复造命令。

## 按任务找代码

| 要处理的事 | 前端入口 | Rust / 配置入口 | 优先检查 |
| --- | --- | --- | --- |
| 登录 / 凭据 / 账户切换 | `src/App.tsx`、`features/auth/` | `commands.rs`、`credential_manager.rs`、`koofr_api/` | 退出时取消任务、清会话和账户隔离 |
| 文件 / 回收站 / 已连接存储 | `features/files/`、`features/places/` | `commands.rs`、`file_ops/`、`koofr_api/` | Rust 路径、mount 和根目录操作校验 |
| 分享链接 | `features/links/`、`services/publicLinks.ts` | `link_commands.rs`、`koofr_api/public_links.rs` | 密码不持久化、创建后配置失败回滚 |
| 普通 / 分卷 / 文件夹传输 | `hooks/useTransferManager.ts`、`features/transfers/` | `transfer/`、`transfer_commands.rs`、`split_commands.rs`、`folder_download.rs` | 续传语义、校验、取消 / 暂停、失败清理 |
| 经典 / 按需同步 | `hooks/useSyncManager.ts`、`features/sync/SyncWorkspace.tsx` | `sync.rs`、`cloud_files.rs`、`commands.rs` | 三方基线、条件删除、范围不重叠、CFAPI 身份 |
| Vault | `features/vault/`、`types/vault.ts` | `vault_commands.rs`、`vault_core/`、`crypto/`、`transfer/vault.rs` | 复用已有实现、Safe Key 边界和互操作测试 |
| 文件预览 | `features/files/FilePreviewModal.tsx`、`preview.ts` | `stream_commands.rs`、`stream_session.rs`、`preview_decode.rs`、`lib.rs` scheme handler | Range、关闭清理、大小上限、Vault 明文内存 |
| 设置 / 桌面 / 工作目录 | `hooks/useAppSettings.ts`、`features/settings/`、`features/transfers/FloatingTransferWindow.tsx` | `settings.rs`、`desktop.rs`、`autostart.rs`、`work_directory.rs` | 启动即生效、托盘退出语义、迁移失败恢复 |
| IPC / 权限 / 安装 / 发布 | `services/koofr.ts`、`types/backend.ts` | `lib.rs`、`capabilities/`、`tauri.conf.json`、`windows/hooks.nsh`；根目录 `scripts/release-forgejo.mjs` | 最小权限、版本一致、标签与产物来源 |

表中前端相对路径除特别说明外位于 `src/`，Rust 相对路径位于 `src-tauri/src/`；capabilities、Tauri 配置和 Windows hooks 位于 `src-tauri/`。

`App.tsx` 负责跨模块协调，全局设置、传输和同步逻辑已有 `src/hooks/` 中的 hooks，不要重新堆回 App。feature 之间不要新增直接依赖。业务 Tauri 调用只通过两个 services 文件；当前 `TitleBar.tsx` 直接调用窗口控制是既有偏差，不是允许其他组件绕过 services 的范例。错误用 `commandErrorMessage` / `isCommandErrorCode`。视觉遵守 [DESIGN.md](../DESIGN.md) 与 `src/theme.ts`。

## 不得破坏的边界

- **凭据**：登录表单的应用专用密码提交给 Rust 后清除；session token 不回传 WebView。只有用户选择保存时才存入 Windows Credential Manager，当前实现服务名 `Koofr-GUI`、账号为邮箱，见 `credential_manager.rs`。不读取或输出真实凭据，不把密码 / token 放进日志、URL、命令行或 fixture。
- **Vault**：Safe Key 只走 Windows 原生提示与 Rust，不经过 IPC / React。保留固定官方 `vault-crypto` 与 rclone 兼容格式，不自创加密。密文路径与密钥留在 Rust，前端只拿显示元数据和短期不透明句柄。导出的 rclone `password` / `password2` 可恢复，按敏感凭据对待；预览明文只在受限、可清零的 Rust 内存。
- **本地 / 远端路径**：Rust 再次校验 mount、操作范围、远端规范路径、本地绝对路径和符号链接。拒绝远端根的破坏性操作。手动上传来自原生选择器授权，下载消费一次性且区分文件 / 文件夹的 auth ID；前端不得决定最终文件名。
- **写入与清理**：手动下载不覆盖，使用专用临时文件 / 目录并稳定避让同名。同步按其三方基线和条件操作协调，不得改成无条件覆盖或删除；不能套用手动下载规则擅改同步语义。清理只处理已经验证归属的精确路径；不要扫描、删除用户的应用数据来“修复”测试。
- **传输**：普通 / Vault 上传是整文件重试；分卷按确认完整的 part 续传，保留裸字节与开放恢复格式；下载才是真正的 Range 字节续传，200 回退安全截断，远端 validator 变化则失败。自动重试仅适用 `network_error`，等待仍可取消。
- **同步与 CFAPI**：仅支持主存储普通目录，不扩展到 Vault / 共享 / 第三方挂载；拒绝范围重叠。首次合并不传播删除，后续用基线防止误删并保留冲突。占位符身份只用不透明 UUID，解除注册 / 卸载先核对自己的 Provider 与根身份。完全退出会停止同步，托盘隐藏不会。
- **IPC / 日志 / 代码加载**：错误仅 `{ code, message }`，安全消息不含路径或响应正文。诊断日志与持续进度事件不带邮箱、文件名、完整路径、密钥 / token 或响应正文；按需任务详情与业务显示数据另走受限命令。不扩大 capabilities，不给 WebView 任意 shell / 文件系统 / 网络能力，不在有 IPC 的 WebView 加载远端脚本。

完整行为解释见 [ARCHITECTURE.md](ARCHITECTURE.md)。若要改协议或加密，先核对官方 Go / Java SDK、Koofr Vault 与 rclone 对应源码；本文件的概述不能替代协议依据。

## 实施与验证

从已有实现做最小连贯改动，不顺手升级依赖或重构无关模块。先读 `package.json` 的真实脚本，再执行：

| 情况 | 命令 / 要求 |
| --- | --- |
| 新环境 | 按 [BUILDING.md](BUILDING.md) 配置 Windows MSVC、Node 与 Rust；`npm ci` 复现依赖 |
| 快循环 | `npm run verify:quick`：lint + SplitUploadDialog 测试 + 前端构建，不代表全测试覆盖 |
| Rust、跨栈或交付前完整验证 | `npm run check`：lint、全部 Vitest（包括发布工具）、前端构建、Rust fmt / Clippy / test |
| 故障定位 | `npm run lint`、`npm run test`、`npm run build`、`npm run check:rust`；按失败阶段定向定位 |
| exe / 安装包任务 | 获得对应任务后再用 `npm run build:desktop` / `npm run build:installer`，遵守 [RELEASING.md](RELEASING.md) |

`npm run dev` 只是浏览器前端，不能证明 Tauri 登录、传输、CFAPI 或原生窗口可用。**不要运行 Computer Use、桌面自动化、UI Automation、SendInput 或同等能力去启动、控制、检查、捕获桌面应用。** GUI、资源管理器和安装卸载交由用户手动验收，报告为“需要人工在本机验证”。

默认 Rust 测试有一项会修改临时 Windows Cloud Files 同步根的 ignored 集成测试；保持跳过，不擅自加 `--ignored`。涉及加密修改需保留 / 补充合成 golden vectors、Unicode / salt / 块边界与篡改拒绝测试，不使用真实 Vault 导出。

文档更新遵守 [CONTRIBUTING.md](../CONTRIBUTING.md)：用户行为改动同步中英文 README 和 `CHANGELOG.md` 的 Unreleased；架构与命令改动同步架构 / 模块说明；构建和发布改动更新对应指南。最后检查 `git diff --check` 和文件清单，确保没有意外改锁文件、版本或生成物。

## 外部影响与交付

没有用户明确要求，不执行 `git commit`、`git push`、打 tag、创建 Issue / PR、触发发布或修改真实 Koofr 账户。既有发布标签不移动；迁移重建的草稿和未确认来源的安装包不视为正式发布。发布脚本 dry run 会检查 HEAD / 标签 / 安装包；失败时按前提排查，不绕过校验。

`AGENTS.md`、`CLAUDE.md`、`.agents/`、`.claude/`、`.omo/` 和 `scripts/release-local.ps1` 是被忽略的本地上下文，永远不提交、不用 `git add -f`。跨机器接手应链接本文件和 [项目进度](PROJECT_STATUS.md)，不依赖这些本地文件的存在。

交付至少写清：行为差异与修改文件、安全影响、实际检查命令及结果、未运行项及原因、需要人工验证的内容和剩余问题。不要把历史通过记录当成本轮测试，不把“已实现”写成“本机 GUI 验收通过”。

## 交给下一位 agent 的任务模板

复制下面内容并填写具体任务；文档已包含背景，无需粘贴整段聊天：

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
