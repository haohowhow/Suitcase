# 贡献指南

感谢你参与改进 Koofr-GUI。本指南说明如何提交 Issue、发起 Pull Request，以及项目对代码、提交信息和测试的要求。

英文版见文档末尾的 [English version](#english-version)。

## 目录

- [行为准则](#行为准则)
- [先看这些](#先看这些)
- [报告 Bug](#报告-bug)
- [提出新功能](#提出新功能)
- [开发环境](#开发环境)
- [Pull Request 流程](#pull-request-流程)
- [代码风格](#代码风格)
- [测试](#测试)
- [提交信息约定](#提交信息约定)
- [文档](#文档)
- [禁止提交的内容](#禁止提交的内容)
- [License 与 DCO](#license-与-dco)
- [English version](#english-version)

## 行为准则

参与本项目请遵守基本准则：尊重他人、假定善意、聚焦技术问题。不接受骚扰、人身攻击或公开他人隐私。项目维护者保留删除评论、关闭 Issue / PR 或封禁账号的权利。

## 先看这些

开始前请确认：

恢复开发或交由代理接手时，请先阅读 [项目进度](docs/PROJECT_STATUS.md) 与 [Agent 接手指南](docs/AGENT_HANDOFF.md)。接手指南包含代码导航、安全限制、验证要求与任务模板；本地 `AGENTS.md` 等被忽略文件不随克隆分发，请勿提交。

1. 已阅读 [README.md](README.md) 与 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。
2. 已搜索[已有 Issue](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/issues)，拟报告的问题可能已在讨论中。
3. 涉及安全漏洞的内容**不得**提交为公开 Issue，请使用 [SECURITY.md](SECURITY.md) 中的私密渠道。

## 报告 Bug

一份完整的 Bug 报告至少包含：

- **Koofr-GUI 版本**（应用界面、About 页或安装包文件名）
- **操作系统**（Windows 10 / 11，位数）
- **重现步骤**（尽可能具体，最好从启动应用开始）
- **实际结果与期望结果**
- **错误消息 / 错误码**（应用返回的稳定错误码；不要附含路径或令牌的完整日志）

若涉及传输问题，请补充：

- 传输类型（普通上传 / 分卷上传 / 单文件下载 / 文件夹下载）
- 文件大小量级（无需精确数值）
- 触发条件（网络中断、应用重启、磁盘空间不足等）

## 提出新功能

请先创建 Issue 讨论，标题使用 `[Feature]` 或 `[Proposal]` 前缀，并说明：

- 要解决的问题
- 至少一个你自己会使用的场景
- 考虑过的替代方案及未采用的原因

**已明确不属于本项目范围**：

- 依赖 Koofr 官方公开桌面客户端 API 的功能（OAuth、第三方存储管理）
- 自定义加密格式（Vault 必须兼容 Koofr / rclone crypt）
- 复用网页 Cookie、内嵌其他客户端密钥

## 开发环境

见 [docs/BUILDING.md](docs/BUILDING.md)。快速开始：

```powershell
git clone https://forgejo.haohaohow.com/sora-admin/Koofr-GUI.git
Set-Location Koofr-GUI
npm ci
npm run dev:desktop
```

## Pull Request 流程

1. **Fork 并创建分支**。分支名使用简短描述性名称，例如 `fix/split-upload-retry` 或 `feat/vault-unlock`。
2. **保持改动聚焦**。一个 PR 只处理一件事；相互独立的改动请拆分为多个 PR。
3. **保持锁文件不变**。不要顺带升级依赖；如需升级，请单独提交 PR 并说明理由。
4. **本地通过全部检查**：

   ```powershell
   npm run check
   ```

   该命令依次执行 ESLint、Vitest、TypeScript 与 Vite 构建、`cargo fmt --check`、Clippy（警告视为错误）和 Rust 单元测试，且必须全部通过。

5. **补充测试**。新增功能模块或修复 Bug 时，应补充对应的 `.test.ts` / `.test.tsx` 或 Rust `#[cfg(test)]` 测试。
6. **更新文档**。用户可见的行为变更需更新 README；架构或安全约定变更需更新 [ARCHITECTURE.md](docs/ARCHITECTURE.md)。
7. **更新 CHANGELOG.md**。在 `## [Unreleased]` 段落下新增条目，遵循 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) 格式；若不存在 `Unreleased` 段落，请新建。
8. **向 `main` 提交 PR**。标题用一行描述改动内容，描述中说明：
   - 动机与关联 Issue
   - 主要改动点
   - 已手动验证的内容
   - 是否存在破坏性变更

## 代码风格

### TypeScript / React

- **严格模式**（`tsconfig` 已启用）。不要使用 `any`；需要时使用 `unknown` 并做类型收窄。
- **函数组件与 hooks**，不使用 class 组件。
- **显式接口类型**放在组件文件顶部或 `types/` 中。
- 命名约定：组件 `PascalCase`，hooks `useXxx`，常量 `SCREAMING_SNAKE_CASE`，其余 `camelCase`。
- **错误处理统一使用辅助函数**：`commandErrorMessage(error, fallback)` 获取安全消息，`isCommandErrorCode(error, code)` 判断错误码。不要直接打印错误或将原始错误对象传入 UI。
- ESLint 配置位于 `eslint.config.js`，执行 `npm run lint` 检查。

### Rust

- **edition 2024**，使用 `rustfmt` 默认配置。
- **Clippy 警告视为错误**（`-D warnings`）。
- **命名约定**：模块与文件使用 `snake_case`，类型与 trait 使用 `PascalCase`，函数与变量使用 `snake_case`。
- **错误类型统一使用 `AppError`**（`src-tauri/src/error.rs`）。返回给前端的错误必须是稳定错误码与安全消息，**不得**包含路径、令牌或响应正文。
- **新增 Tauri 命令必须**：
  - 校验挂载点 ID 与远程路径（拒绝 `.`、`..`、NUL 及超长段）
  - 校验本地路径（绝对路径、已存在、非符号链接）
  - 限定操作范围，禁止越权访问其他账户或目录
  - 在 `commands.rs` 或对应模块中注册

### 提交前自检清单

- [ ] `npm run lint` 无错误
- [ ] `npm run test` 全部通过
- [ ] `npm run check:rust` 全部通过
- [ ] 新增或修改的行为有测试覆盖
- [ ] 相关文档已同步更新
- [ ] CHANGELOG 已更新
- [ ] 未引入 `console.log`、`dbg!` 或临时调试代码

## 测试

- **前端**：Vitest + jsdom + Testing Library。测试文件与源文件位于同一目录，命名为 `*.test.ts` 或 `*.test.tsx`。
- **Rust**：单元测试使用 `#[cfg(test)] mod tests`，或作为独立测试文件放在 `src-tauri/src/**/xxx_tests.rs`。
- 涉及网络请求的 Rust 测试应尽量使用 mock，不要在测试中访问真实 Koofr 账户。
- 涉及本地文件系统的测试请使用 `tempfile` 等工具，不要硬编码路径。

## 提交信息约定

本项目不强制要求 Conventional Commits，但推荐使用以下格式：

```text
<type>: <subject>

<body>

<footer>
```

常用 `type`：`feat`、`fix`、`refactor`、`docs`、`test`、`chore`、`perf`、`build`。

示例：

```text
fix(transfer): reset progress correctly after pause

Progress was previously reset to 0% when the user paused, because the
frontend read the pending checkpoint before the Rust side had time to
emit the paused progress event.

Closes #42
```

- 主语可使用英文或中文，同一仓库内保持一致即可。
- 标题行不超过 72 个字符。
- 关联 Issue 使用 `Closes #N` / `Refs #N`。

## 文档

不同改动需要同步的文档：

| 改动 | 需要更新 |
| --- | --- |
| 用户可见的功能变化 | `README.md`、`README.en.md`、`CHANGELOG.md` |
| 新增 Tauri 命令 | `src-tauri/README.md` |
| 架构、数据流或安全边界变化 | `docs/ARCHITECTURE.md` |
| 构建流程变化 | `docs/BUILDING.md` |
| 发布流程变化 | `docs/RELEASING.md` |
| 设计 token 或 UI 约定变化 | `DESIGN.md` |
| 接手流程、安全约束或模块入口变化 | `docs/AGENT_HANDOFF.md`；阶段进度变化另行更新 `docs/PROJECT_STATUS.md` |

## 禁止提交的内容

- `node_modules/`、`dist/`、`src-tauri/target/`、`src-tauri/gen/`、`*.tsbuildinfo`
- 真实的 Koofr 账号、密码、访问令牌或 OAuth 密钥
- 本地路径、用户名、内网 IP 等个人信息
- 未压缩的大型二进制文件（截图请先压缩）
- IDE 工作区文件（除非项目根 `.gitignore` 明确允许）

## License 与 DCO

- 本项目采用 [MIT License](LICENSE)。
- 提交 PR 即表示你同意以相同的 MIT 协议发布你的贡献。
- 你的代码必须由你本人编写，或你有权以 MIT 协议授权。请勿提交来源不明的代码。

---

## English version

Thanks for your interest in improving Koofr-GUI. The full rules are above (in Chinese); this summary covers the essentials.

Contributors returning to the project, or coding agents taking over, should read [Project status](docs/PROJECT_STATUS.md) and the [Agent handoff guide](docs/AGENT_HANDOFF.md) (both in Chinese). Local ignored instruction files are not part of a clone and must not be committed.

1. **Search existing issues first**; do not file duplicates.
2. **Report security bugs through [SECURITY.md](SECURITY.md), never in public issues.**
3. **Fork, create a branch, and open a PR against `main`.** One logical change per PR.
4. **`npm run check` must pass** before requesting review; it runs ESLint, Vitest, the TypeScript and Vite build, `cargo fmt --check`, Clippy (warnings as errors), and Rust unit tests.
5. **Every new Tauri command must validate** mount IDs, remote paths (rejecting `.`, `..`, NUL, and overlong segments), local paths (absolute, existing, non-symlink), and operation scope on the Rust side.
6. **Error messages returned to the frontend must not expose paths, tokens, or response bodies**; use `AppError` and stable error codes.
7. **Update documentation** (README, ARCHITECTURE, CHANGELOG) for user-visible or architectural changes.
8. **Do not bundle dependency upgrades** into feature PRs; open a separate PR for updates.
9. **License**: contributions are released under [MIT](LICENSE). By opening a PR you certify that you have the right to license your work under MIT.
