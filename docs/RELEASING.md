# 发布流程

项目托管于 [自建 Forgejo](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI)。目前没有 Windows runner，正式发布在 Windows 本机完成检查和构建，再上传 Forgejo Release 草稿。推送标签不会自动构建或发布。

## 发布策略

- 只发 Windows x64 NSIS 安装包，暂不做代码签名。
- 唯一发布渠道是 [本仓库 Releases](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases)。私有仓库需要登录并有访问权限才能下载。
- 每次附上 `SHA256SUMS`，只包含本次安装包的 SHA-256；不上传账户数据、日志或凭据。
- 上传脚本只创建草稿，拒绝覆盖已发布 Release 或已有附件。人工检查附件与说明后在 Forgejo 发布。
- 仓库 Actions 暂时关闭，旧 GitHub 工作流已移除。将来配置专用 Windows runner 后再启用；不能假定 Forgejo 提供 GitHub 的 `windows-latest` 托管环境。

## 版本号规则

遵守 [SemVer](https://semver.org/spec/v2.0.0.html)。以下三个手写版本源必须一致：

- `package.json` 的 `version`。
- `src-tauri/tauri.conf.json` 的 `version`。
- `src-tauri/Cargo.toml` 的 `[package].version`。

锁文件 `package-lock.json` 与 `src-tauri/Cargo.lock` 应由工具同步，不手写修改。标签为 `v<version>`，发布时 HEAD 必须与标签指向同一提交。已发布标签不移动、不删除。

## 发一个版本

### 1. 准备版本与说明

更新三个版本源与锁文件，将 `CHANGELOG.md` 的 Unreleased 内容整理到 `## [<version>] - YYYY-MM-DD` 段落。迁移后的应用链接等代码变更应通过新版本分发，不重新打包覆盖历史版本。

### 2. 检查并构建

在 Windows MSVC 环境、仓库根目录执行：

```powershell
npm ci
npm run check
npm run build:installer
```

构建产物在 `src-tauri/target/release/bundle/nsis/`，实际文件名来自 Tauri `productName`，例如 `Koofr GUI_<version>_x64-setup.exe`。使用本机人工验证安装、启动和关键操作；代理端检查不能替代 GUI 验收。

### 3. 提交和推送标签

确认工作区、版本和测试结果后，由维护者提交版本变更，再执行：

```powershell
git tag v<version>
git push origin main
git push origin v<version>
```

标签推送只保存源码版本；此时尚未创建 Release。

### 4. 检查上传计划

```powershell
node scripts/release-forgejo.mjs --dry-run
```

脚本验证三个版本源、CHANGELOG、标签与 HEAD、安装包名称，并计算 SHA-256。Dry run 不联网、不上传、不需要 token。正式上传还要求干净工作区。请从刚通过检查的源码构建；脚本不能证明一个遗留安装包的源码来源。

### 5. 上传 Release 草稿

在 Forgejo 创建仅限本仓库、具有 `write:repository` 权限的访问令牌。令牌只通过本地环境变量传给脚本，不写入仓库、不作为命令行参数，也不发到聊天中。PowerShell 可用隐藏输入：

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

脚本会核对 Forgejo 上已有标签的提交，创建草稿，再上传安装包和 `SHA256SUMS`；不创建标签、不提交、不推送、不自动发布。认证请求固定到本服务，拒绝跟随重定向，错误仅显示状态类别，不打印令牌或服务端正文。

如果上传中断，草稿会保留。检查现有附件后，在 Forgejo 删除不完整的草稿再重试；不要覆盖已有正式 Release。API token 若被撤销或权限不足，请在本地重新配置。

### 6. 人工发布

打开 [Releases](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases)，核对标签、说明、两个附件、安装包版本与哈希。人工安装验收后发布草稿。没有完成 GUI 验收的包保留为草稿。

## 用户如何校验安装包

1. 从本仓库 Forgejo Releases 下载，确认所有者为 `sora-admin`、仓库为 `Koofr-GUI`。
2. 确认标签、安装包文件名与 CHANGELOG 版本一致。
3. 同时下载 `SHA256SUMS`，计算并比对完整 SHA-256：

   ```powershell
   Get-FileHash 'Koofr GUI_<version>_x64-setup.exe' -Algorithm SHA256
   ```

哈希确认文件与该发布页面公布的文件一致，不等同于代码签名。发布包仍可能触发 Windows SmartScreen 未知发布者提示。

## 历史版本迁移

Git 分支、标签和提交历史不包含 Release 附件、Issue、PR 或评论。这些数据需要从旧服务或独立备份恢复。详见 [FORGEJO_MIGRATION.md](FORGEJO_MIGRATION.md)。

根据 Git / CHANGELOG 重建的历史 Release 说明必须标记来源，不能宣称恢复了原始页面、发布日期或附件。无法证明是原发布附件的本地构建包，不作为原附件上传；重新构建的版本应明确标注来源并完成验收。

## 紧急撤回一个版本

在 Release 说明顶部标记“已撤回，不要使用”，删除危险附件或改名加 `.YANKED`，在 CHANGELOG 记录原因，并尽快发布修复版本。保留历史标签与 Release 记录。安全问题通过 [SECURITY.md](../SECURITY.md) 中的私密邮件渠道协调披露。
