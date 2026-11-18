# Forgejo 迁移记录

迁移日期：2026-10-12。

目标仓库：[sora-admin/Koofr-GUI](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI)。迁移时 Forgejo 版本为 12.0.4，目标为空的私有仓库。仓库保持私有设置；下载与协作需要相应权限。

## 已迁入的 Git 数据

- 迁入本地的 `main`、`develop`、`gitbutler/workspace` 三个分支，以及 `v1.0.0` 至 `v2.6.0` 共 22 个现有版本标签。
- 非浅克隆历史以原对象 ID 推送，25 个分支 / 标签引用均与目标逐一核对一致。未改写提交，也未移动版本标签。
- `origin` 指向 Forgejo，旧 GitHub 地址保留为 `github-old`。三个本地分支均跟踪 Forgejo 对应分支。
- stash、代理内部引用与本地指导文件未上传。迁移前的 `git bundle --all` 备份保存在本机 `.git/migration-backup/`，并通过 `git bundle verify` 校验。

`gitbutler/workspace` 为旧远端已有的工作分支，迁移时一并保留。该分支不应用于正式发布。

## 历史 Release 与附件

旧仓库、Release 与 Issue 的 GitHub 公开 API 在迁移时均返回 404，无法直接导出原记录。Git 标签不包含 Release 页面或附件。

依据本地 CHANGELOG 重建了 22 个历史 Release **草稿**，标题与说明均明确标记为 migration reconstruction；`v1.2.0` 的说明取自该版本标签内的 CHANGELOG。原始 Release 说明、发布日期与发布者信息未恢复。草稿创建日期为本次迁移日期。

另保存了以下本地安装包候选，每个候选均重命名为 `*-recovery-unverified.exe`，上传至对应私有草稿，并附迁移时生成的 `SHA256SUMS`：

| 版本 | 候选来源 | 边界 |
| --- | --- | --- |
| 2.1.0 | 本机 Downloads | 内嵌产品版本一致；无下载来源记录，无法核对原发布哈希 |
| 2.2.0 | 本机 Downloads | 内嵌产品版本一致；无下载来源记录，无法核对原发布哈希 |
| 2.3.0 | 本机 Downloads | 内嵌产品版本一致；无下载来源记录，无法核对原发布哈希 |
| 2.5.0 | 本地 NSIS 构建输出 | 内嵌产品版本一致；无法认定为原 GitHub 发布附件 |

上述候选未执行、未做 GUI 验收，也未作为正式版本发布。原文件与额外的本地 2.2.0 构建包继续保留在本机并另行备份。候选副本、校验文件与恢复清单保存在 `.git/migration-backup/`，不提交到源码仓库。已从 Forgejo 重新下载四个候选与四个校验文件，核对字节与 SHA-256，均与本地恢复副本一致。

迁移时生成的 SHA-256 仅用于验证保存 / 上传前后的字节一致性，不能证明原发布来源。在人工确认来源并完成本机安装验收之前，历史草稿保持草稿状态。其余版本未找回安装包。

## Issue、PR 与其他平台数据

未获得旧 Issue、PR、评论、标签、里程碑与 Wiki 的导出，也未依据 Git 提交伪造这些记录或原作者身份。已有合并提交继续保留在 Git 历史中，但不能据此还原 PR 讨论。

若日后恢复 GitHub 访问或找到独立导出，应先保存导出，再检查 Forgejo 的迁移 / 导入工具是否支持导入已有仓库。不得为了导入元数据而覆盖当前 Git 历史或重复创建记录。

## 托管与发布调整

- 应用内主页、下载与问题反馈入口改为 Forgejo；中英文 README、构建指南与贡献链接同步更新。
- Issue / PR 模板移至 `.forgejo/`，安全报告改为发送至 `zenwinhow@outlook.com`。
- 无 Windows runner，仓库 Actions 暂时关闭，原 GitHub 工作流已移除。正式发布改为在 Windows 本地完成检查与构建，再上传 Release 草稿，见 [RELEASING.md](RELEASING.md)。
- Koofr 官方 SDK、Vault 依赖与第三方源码的 GitHub 地址保持原有来源，本次未迁移第三方项目。
- 应用的认证、传输、加密、缓存与 IPC 权限均未改变。入口变更需随新版本安装包分发，已安装的旧版本仍包含旧链接。

源码与文档调整随迁移提交同步至 Forgejo `main`。已安装应用中的新入口仍需通过后续新版本安装包生效；本次迁移未发布新的安装包。

## 验证结果

- `npm run check` 通过：91 项前端与发布工具测试、前端生产构建、Rust fmt、Clippy 与 123 项 Rust 测试。1 项会修改 Windows Cloud Files 同步根的集成测试按配置跳过。
- 发布工具最后补充的响应正文脱敏检查已通过定向测试，ESLint 复查通过；`git diff --check` 通过。
- Forgejo 设置复查确认：私有仓库、默认分支 `main`、Actions 关闭、22 个 Release 均为草稿、8 个恢复附件下载校验通过。
- 未构建或发布新的正式安装包。当前本机缺少 2.6.0 安装包，上传工具的 dry run 正确拒绝了缺失产物。
- GUI、原安装包来源与安装验收需人工在本机验证。
