# 发布流程

Koofr-GUI 用 GitHub Actions 在推送版本标签时构建和发布 Windows NSIS 安装包。工作流在 [`.github/workflows/release.yml`](../.github/workflows/release.yml)。

## 目录

- [发布策略](#发布策略)
- [版本号规则](#版本号规则)
- [发一个版本](#发一个版本)
- [手动本地发布](#维护者本地发布仅限本地测试)
- [用户如何校验安装包](#用户如何校验安装包)
- [紧急撤回一个版本](#紧急撤回一个版本)

## 发布策略

- **平台**：只发 Windows x64 NSIS 安装包。
- **签名**：**不做代码签名**。用户下载后 Windows SmartScreen 可能显示"未知发布者"警告。这是有意的成本 / 收益权衡，详见 [SECURITY.md](../SECURITY.md#已知的非漏洞行为)。
- **触发**：推送符合 `v*` 的 Git 标签自动触发工作流。
- **Secrets**：不需要任何 GitHub Actions secrets（不签名、不推包管理器）。
- **发布渠道**：**只**通过本仓库的 [GitHub Releases 页面](https://github.com/zenwinhow/Koofr-GUI/releases)。不使用镜像、CDN、第三方包管理器。

## 版本号规则

遵守 [SemVer 2.0](https://semver.org/spec/v2.0.0.html)：

- **MAJOR**：破坏性变更（API、命令签名、检查点格式、数据布局不兼容）。
- **MINOR**：新增功能，向后兼容。
- **PATCH**：Bug 修复，向后兼容。

**三个手写版本源与生成的锁文件镜像**的版本号必须完全一致：

- **手写版本源**：
  - `package.json` 中的 `version` 字段。
  - `src-tauri/tauri.conf.json` 中的 `version` 字段。
  - `src-tauri/Cargo.toml` 中的 `[package].version` 字段。
- **生成的锁文件镜像**（在执行构建或依赖更新时自动同步）：
  - `package-lock.json`（由 `package.json` 自动生成）。
  - `src-tauri/Cargo.lock`（由 `src-tauri/Cargo.toml` 自动生成）。
- **Git 标签**：
  - 推送的 Git 标签必须为 `v<version>` 格式。

工作流会校验 Git 标签和前三处手写版本源完全一致，不一致则拒绝构建。

## 发一个版本

### 1. 更新版本号

假设要发某个版本（例如 `<version>`）：

```powershell
# package.json
# src-tauri/Cargo.toml
# src-tauri/tauri.conf.json
# 三个文件的 version 全改成 <version>
```

（可以用编辑器全局搜索替换，注意别改到 `Cargo.lock`，那个 cargo 会自动更新。）

### 2. 更新 CHANGELOG

在 [`CHANGELOG.md`](../CHANGELOG.md) 里加一段：

```markdown
## [<version>] - YYYY-MM-DD

### Changed
- xxx

### Fixed
- yyy

### Security
- zzz（有则加，无则删）
```

日期用 `YYYY-MM-DD`（发布日期，不是提交日期）。格式遵守 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)。

### 3. 本地质量检查

```powershell
npm ci
npm run verify:full
```

（`verify:full` 等价于 `npm run check`。）

### 4. 提交 + 打标签

```powershell
git.exe add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json CHANGELOG.md
git.exe commit -m "Prepare <version> release"
git.exe tag v<version>
git.exe push origin main
git.exe push origin v<version>
```

### 5. 观察 GitHub Actions

去 [Actions 页面](https://github.com/zenwinhow/Koofr-GUI/actions) 看 `Publish Windows release` 工作流。它会：

1. 校验 tag 和三个版本源一致。
2. 装依赖 + 跑全套质量检查（等同 `npm run check`）。
3. 构建未签名的 NSIS 安装包。
4. 传到同名的 GitHub Release 上。

如果检查或构建失败，工作流不会上传发布资产。修复问题后：

- **如果标签还没被下载过**：可以强制删除标签重新打（`git.exe tag -d v<version>; git.exe push origin :refs/tags/v<version>`）。但一般不推荐，因为破坏了不可变性。
- **建议**：直接发一个 patch 版本（例如 `<new_patch_version>`）。

### 6. 编辑 Release 说明

工作流会创建 Release 但不填详细说明。手动到 Release 页面：

- 把 CHANGELOG 里对应版本段落复制到 Release 描述里。
- 顶部加一段"下载校验方式"，指向 [用户如何校验安装包](#用户如何校验安装包)。
- 如果是 Pre-release，勾"Set as a pre-release"。

## 维护者本地发布（仅限本地测试）

一般情况下不推荐使用。如果需要进行本地测试或在 CI 不可用时构建临时测试包：

仓库中包含一个被 Git 忽略的维护者本地脚本 `scripts/release-local.ps1`（如果存在）。该脚本会在本地执行干净构建并生成安装包，但**不会提交代码、不会打标签、也不会推送 Release**。构建产物位于 `src-tauri/target/release/bundle/nsis/` 目录下。

在分发本地构建的安装包前，请务必注意：本地构建的安装包与 CI 构建的产物可能存在差异（如 rustc 或 Node.js 版本、依赖锁文件状态、环境变量等）。正式发布版本必须通过 GitHub Actions 构建。

## 用户如何校验安装包

因为不做代码签名，用户下载后至少应该：

1. **确认下载来源**：必须为 `https://github.com/zenwinhow/Koofr-GUI/releases`（注意仓库地址和所有者）。
2. **确认标签和文件名一致**：例如标签 `v<version>` 对应的安装包文件名通常为 `Koofr-GUI_<version>_x64-setup.exe`。
3. **对照 CHANGELOG 里的版本段落**确认功能符合预期。
4. **可选**：校验 SHA-256。用户可以使用 PowerShell 命令行计算下载文件的本地哈希值：

   ```powershell
   Get-FileHash "Koofr-GUI_<version>_x64-setup.exe" -Algorithm SHA256
   ```

   需要注意的是，`Get-FileHash` 仅用于计算本地文件的哈希值。只有当发布页面上显式且可信地公布了对应的哈希值时，才能进行比对校验；若未公布，则只能通过确认下载来源（即源头可信度）来作为主要的安全验证手段。

如果任何一步不对，**不要运行安装包**，请到 Issues 里反馈。

## 紧急撤回一个版本

如果发出去的版本有严重问题（数据损坏、凭据泄露之类的）：

1. **立即**在对应 Release 顶部加显眼警告："⚠️ 已撤回，不要使用此版本"。
2. 在 Release Assets 上加 `.YANKED` 后缀 rename，或者直接删除 Assets。
3. 在 CHANGELOG 里对应版本下加 `### Yanked` 段落，写清楚撤回原因。
4. 尽快发一个修复版本（PATCH，或者必要时 MINOR）。
5. 如果是安全问题，走 [SECURITY.md](../SECURITY.md) 的 Coordinated Disclosure 流程发 Security Advisory。
6. **不要 `git.exe tag -d` 已发布的标签**，标签保持存在但不再是"当前推荐版本"。

被撤回的 Release 不删除，只做标记，这样已经下载了的用户还能识别自己拿到的是哪个版本。
