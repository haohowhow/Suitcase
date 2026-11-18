# 安全策略

## 目录

- [支持的版本](#支持的版本)
- [报告漏洞](#报告漏洞)
- [响应时间](#响应时间)
- [披露策略](#披露策略)
- [安全设计边界](#安全设计边界)
- [已知的非漏洞行为](#已知的非漏洞行为)
- [English version](#english-version)

## 支持的版本

仅为最新稳定版本提供安全修复：

| 版本 | 是否支持 |
| --- | --- |
| 最新稳定版 | ✅ |
| 其他历史版本 | ❌ 请升级至最新版本 |

发布包**未做代码签名**，请仅从[官方 Releases 页面](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases)下载。

## 报告漏洞

**请勿通过公开 Issue 报告漏洞。**

请通过以下私密渠道联系：

- **邮件**：[zenwinhow@outlook.com](mailto:zenwinhow@outlook.com)。首次联系请勿附上真实凭据或敏感文件；如需发送加密附件，请先与维护者确认加密方式。

报告请尽可能包含：

- 受影响的 Koofr-GUI 版本或 commit
- 受影响的组件（前端 / Rust / 打包 / 分发）
- 重现步骤（建议附带最小复现示例）
- 潜在影响（本地权限提升、数据泄露、传输劫持等）
- 建议的修复方向（可选）

**请勿**在报告中包含：

- 你的真实 Koofr 账号密码或令牌
- 他人的个人信息
- 生产环境中的敏感文件

## 响应时间

本项目为个人维护的开源项目，响应时间仅能做到尽力而为：

| 阶段 | 目标 |
| --- | --- |
| 收到确认 | 5 个工作日内 |
| 初步评估（是否属于漏洞、严重程度） | 10 个工作日内 |
| 修复 / 缓解 | 视严重程度而定；Critical / High 级别的目标为 30 天内发布补丁 |

如超过上述时间仍未收到回复，请通过邮件再次联系。

## 披露策略

采用 **协调披露**（Coordinated Disclosure）：

1. 收到报告后进行私下确认与评估。
2. 若确认为漏洞，双方共同商定披露时间线，通常为 **90 天**内发布修复。
3. 修复发布后，在 CHANGELOG 与 Forgejo Release 说明中公开细节；经报告者同意后可致谢。
4. 报告者请在**修复发布前**不要公开细节。

## 安全设计边界

Koofr-GUI 的信任模型：

```text
┌─────────────────────────────────────────────────────┐
│ 用户（持有 Koofr 账号与本机 Windows 用户会话）        │
└────────────────────┬────────────────────────────────┘
                     │ WebView2 UI
                     ▼
┌─────────────────────────────────────────────────────┐
│ React + TypeScript（可信，但仅能调用受限命令）        │
│ - 不持有密码、令牌或 Vault Safe Key                  │
│ - 不通过 XHR / fetch 直连 koofr.net                  │
└────────────────────┬────────────────────────────────┘
                     │ typed Tauri IPC
                     ▼
┌─────────────────────────────────────────────────────┐
│ Rust core（信任边界）                                │
│ - Koofr HTTP 请求、凭据管理、路径校验                │
│ - Windows Credential Manager                        │
│ - 磁盘 IO                                            │
└─────────────────────────────────────────────────────┘
```

以下威胁在防护范围内：

- **前端逃逸**：WebView 或前端注入代码尝试读取密码、令牌，或直接调用 Koofr API。
- **路径穿越**：前端传入恶意路径，试图读写沙箱之外的文件。
- **凭据泄露**：日志、错误消息、事件或崩溃报告中意外携带密码、令牌、本地 / 远程路径或服务端响应正文。
- **传输覆盖**：下载覆盖用户已有文件。
- **不完整传输被误判为完整**：分卷上传或断点续传的校验被绕过。
- **本机其他账户读取凭据**：Windows Credential Manager 的作用域限于当前用户，跨用户读取需要提权，但本项目不会主动扩大权限范围。

以下情况**不在**当前防护范围内（本机 Windows 用户被视为可信）：

- 同一 Windows 用户下运行的其他程序读取 `%LOCALAPPDATA%\net.koofr.desktop.gui\` 中的**非凭据**文件（设置、检查点、缓存）。这些文件不包含密码、令牌或文件内容。
- 具有本机管理员权限的攻击者读取 Windows Credential Manager 中的密码。该场景属于操作系统层面的信任模型，超出应用职责范围。
- 用户主动分享自己账户的 refresh_token 或 session token。

## 已知的非漏洞行为

以下情况**不视为漏洞**，请勿作为安全报告提交：

- **安装包未签名 / SmartScreen 警告**：已在 README 与 RELEASING.md 中明确记录。不签名是有意的成本与收益权衡。
- **`.koofr-part-*` 临时文件可被本机同用户程序读取**：临时文件位于用户下载目录中，Windows 文件权限已提供足够保护；下载完成后会重命名为最终文件。
- **Koofr 会话令牌保存在 Rust 进程内存中**：进程内存对同一 Windows 用户的调试器可见，属于操作系统信任模型。
- **前端可访问当前挂载点、目录列表与传输进度**：这些是 UI 必需信息，前端本就有权访问。
- **应用未做证书固定（certificate pinning）**：依赖系统 CA 存储与 rustls 默认校验。Koofr 更换证书链，或用户系统 CA 被本人修改，均属预期行为。

## English version

**Do not open public issues for security vulnerabilities.** Contact the maintainer privately:

- **Email**: [zenwinhow@outlook.com](mailto:zenwinhow@outlook.com). Do not include real credentials or sensitive files in your first message. Agree on encryption with the maintainer before sending sensitive attachments.

Only the latest stable release is supported. Older versions must be upgraded to receive fixes.

Response targets (best effort, individually maintained project):

- Acknowledgement within 5 business days
- Triage within 10 business days
- Critical and High fixes within 30 days where feasible

Coordinated disclosure with a 90-day default embargo. Please keep details private until the fix ships. Credit is given on request in the CHANGELOG and Forgejo release notes.

**In scope**: frontend escape, path traversal, credential leaks in logs, error messages, events, or crash reports, overwriting of user files, and bypassed integrity checks in split upload or byte resume.

**Out of scope**: unsigned binaries (documented), same-user local processes reading non-credential state, admin-level attackers on the same machine, users willingly sharing their own tokens, and the absence of certificate pinning.

The full policy is above (in Chinese).
