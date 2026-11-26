# 发布流程

项目托管于[自建 Forgejo](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI)。当前没有 Windows runner，正式发布在 Windows 本机完成检查与构建，再上传为 Forgejo Release 草稿。推送标签不会自动触发构建或发布。

## 发布策略

- 仅发布 Windows x64 NSIS 安装包，暂不做代码签名。
- 唯一的发布渠道是[本仓库 Releases](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases)。私有仓库需要登录并具备访问权限才能下载。
- 每次发布附带 `SHA256SUMS`，仅包含本次安装包的 SHA-256；不上传账户数据、日志或凭据。
- 上传脚本仅创建草稿，拒绝覆盖已发布 Release 或已有附件。人工检查附件与说明后，再在 Forgejo 发布。
- 仓库 Actions 暂时关闭，原 GitHub 工作流已移除。待配置专用 Windows runner 后再启用；不能假定 Forgejo 提供与 GitHub `windows-latest` 等价的托管环境。

## 版本号规则

遵循 [SemVer](https://semver.org/spec/v2.0.0.html)。以下三个手工维护的版本源必须保持一致：

- `package.json` 的 `version`。
- `src-tauri/tauri.conf.json` 的 `version`。
- `src-tauri/Cargo.toml` 的 `[package].version`。

锁文件 `package-lock.json` 与 `src-tauri/Cargo.lock` 应由工具同步，不得手工修改。标签格式为 `v<version>`；发布时 HEAD 必须与标签指向同一提交。已发布的标签不得移动或删除。

## 发布新版本

### 1. 准备版本与说明

更新三个版本源与锁文件，并将 `CHANGELOG.md` 中 `Unreleased` 的内容整理到 `## [<version>] - YYYY-MM-DD` 段落。迁移后的应用链接等代码变更应通过新版本分发，不得重新打包覆盖历史版本。

### 2. 检查并构建

在 Windows MSVC 环境、仓库根目录执行：

```powershell
npm ci
npm run check
npm run build:installer
```

构建产物位于 `src-tauri/target/release/bundle/nsis/`，实际文件名来自 Tauri 的 `productName`，例如 `Suitcase_<version>_x64-setup.exe`。请在本机验证安装、启动与关键操作；自动化检查不能替代 GUI 验收。

### 3. 提交并推送标签

确认工作区、版本与测试结果后，提交版本变更，再执行：

```powershell
git tag v<version>
git push origin main
git push origin v<version>
```

推送标签仅保存源码版本；此时尚未创建 Release。

### 4. 检查上传计划

```powershell
node scripts/release-forgejo.mjs --dry-run
```

脚本会校验三个版本源、CHANGELOG、标签与 HEAD、安装包名称，并计算 SHA-256。Dry run 不联网、不上传、不需要 token。正式上传还要求工作区干净。请使用刚通过检查的源码构建产物；脚本无法证明遗留安装包的源码来源。

### 5. 上传 Release 草稿

在 Forgejo 创建仅限本仓库、具有 `write:repository` 权限的访问令牌。令牌只能通过本地环境变量传给脚本，不得写入仓库、作为命令行参数或发送到聊天中。PowerShell 中可使用隐藏输入：

```powershell
$releaseSecret = Read-Host 'Forgejo access token' -AsSecureString
$releaseCredential = [System.Net.NetworkCredential]::new('', $releaseSecret)
try {
    $env:FORGEJO_TOKEN = $releaseCredential.Password
    node scripts/release-forgejo.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Release upload failed; inspect the draft before retrying.' }
} finally {
    Remove-Item Env:FORGEJO_TOKEN -ErrorAction SilentlyContinue
    $releaseCredential = $null
    $releaseSecret = $null
}
```

脚本会核对 Forgejo 上已有标签对应的提交，创建草稿，再上传安装包与 `SHA256SUMS`；不会创建标签、提交、推送或自动发布。认证请求固定发往本服务，拒绝跟随重定向，错误仅显示状态类别，不打印令牌或服务端响应正文。

若上传中断，草稿会保留。检查现有附件后，在 Forgejo 删除不完整的草稿再重试；不得覆盖已有的正式 Release。若 API token 被撤销或权限不足，请在本地重新配置。

### 6. 人工发布

打开 [Releases](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases)，核对标签、说明、两个附件、安装包版本与哈希。完成人工安装验收后再发布草稿。未完成 GUI 验收的包应保留为草稿。

## 用户如何校验安装包

1. 从本仓库 Forgejo Releases 下载，确认所有者为 `sora-admin`、仓库为 `Koofr-GUI`。
2. 确认标签、安装包文件名与 CHANGELOG 版本一致。
3. 同时下载 `SHA256SUMS`，计算并比对完整 SHA-256：

   ```powershell
   Get-FileHash 'Suitcase_<version>_x64-setup.exe' -Algorithm SHA256
   ```

哈希校验只能确认文件与发布页面公布的文件一致，不等同于代码签名。发布包仍可能触发 Windows SmartScreen 未知发布者提示。

## 历史版本迁移

Git 分支、标签与提交历史不包含 Release 附件、Issue、PR 或评论。这些数据需从旧服务或独立备份恢复，详见 [FORGEJO_MIGRATION.md](FORGEJO_MIGRATION.md)。

依据 Git / CHANGELOG 重建的历史 Release 说明必须标明来源，不得声称恢复了原始页面、发布日期或附件。无法证明为原发布附件的本地构建包，不得作为原附件上传；重新构建的版本应明确标注来源并完成验收。

## 紧急撤回版本

在 Release 说明顶部标记“已撤回，不要使用”，删除危险附件或将其改名为加 `.YANKED` 后缀，并在 CHANGELOG 中记录原因，同时尽快发布修复版本。保留历史标签与 Release 记录。安全问题请通过 [SECURITY.md](../SECURITY.md) 中的私密邮件渠道协调披露。
