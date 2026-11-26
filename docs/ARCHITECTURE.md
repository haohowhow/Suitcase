# 架构说明

本文档说明 Koofr-GUI 的整体架构、数据流、传输恢复设计与安全边界，适合需要了解整体结构或评估安全性的读者。

## 目录

- [顶层结构](#顶层结构)
- [进程与线程模型](#进程与线程模型)
- [数据流](#数据流)
- [核心模块](#核心模块)
  - [前端 `src/`](#前端-src)
  - [Rust 后端 `src-tauri/src/`](#rust-后端-src-taurisrc)
- [关键设计](#关键设计)
  - [认证与会话](#认证与会话)
  - [进程生命周期](#进程生命周期)
  - [Koofr Vault](#koofr-vault)
  - [文件预览](#文件预览)
  - [路径与操作校验](#路径与操作校验)
  - [传输管线](#传输管线)
  - [单文件下载：字节级续传](#单文件下载字节级续传)
  - [分卷上传设计](#分卷上传设计)
  - [文件夹递归下载](#文件夹递归下载)
  - [网络错误自动恢复](#网络错误自动恢复)
  - [应用工作目录](#应用工作目录)
  - [元数据缓存](#元数据缓存)
  - [诊断日志](#诊断日志)
  - [凭据存储](#凭据存储)
  - [错误模型](#错误模型)
- [设置驱动的运行时行为](#设置驱动的运行时行为)
- [本地文件夹同步](#本地文件夹同步)
- [事件与命令目录](#事件与命令目录)
- [本地数据布局](#本地数据布局)
- [未实现的部分](#未实现的部分)

## 顶层结构

```text
┌──────────────────────────────────────────────────────┐
│  WebView2（Chromium 内核）                            │
│  ┌────────────────────────────────────────────────┐  │
│  │ React 19 + TypeScript                          │  │
│  │  - features/ 按功能划分                        │  │
│  │  - services/ 类型化 Tauri 命令封装             │  │
│  │  - 仅调用受限命令，不接触密码或令牌            │  │
│  └────────────────────────────────────────────────┘  │
└─────────────────────┬────────────────────────────────┘
                      │ Tauri IPC (typed commands + events)
                      ▼
┌──────────────────────────────────────────────────────┐
│  Rust 主进程 (tokio 多线程运行时)                    │
│  ┌────────────────────────────────────────────────┐  │
│  │ Tauri Builder + AppState                       │  │
│  │  - KoofrApi (reqwest / rustls)                 │  │
│  │  - TransferManager                             │  │
│  │  - TransferCheckpointStore                     │  │
│  │  - SyncManager (SQLite + watcher/coordinator)  │  │
│  │  - Cloud Files API provider (Windows on-demand) │  │
│  │  - SettingsStore                               │  │
│  │  - MetadataCache                               │  │
│  │  - CredentialManager (Windows Credentials)     │  │
│  │  - LocalAccessManager                          │  │
│  └────────────────────────────────────────────────┘  │
└─────────────────────┬────────────────────────────────┘
                      │ HTTPS (rustls, cert 校验)
                      ▼
              ┌───────────────────┐
              │  Koofr REST API   │
              │  app.koofr.net    │
              └───────────────────┘
```

技术栈基于 Tauri v2、React 19、TypeScript 与 Rust。

## 进程与线程模型

- **多进程**：包含一个 Tauri 主进程（Rust），以及由 WebView2 Runtime 管理的多个 WebView2 子进程（渲染、GPU、网络等）。
- **主线程**：Tauri 事件循环与 UI 事件调度。
- **WebView 窗口**：`main` 持有完整应用状态；可选的 `transfer-float` 是固定尺寸、置顶且跳过任务栏的轻量状态窗口，只接收脱敏的聚合传输快照。
- **tokio runtime**：`rt-multi-thread`，异步 HTTP、异步磁盘 IO、传输任务与进度事件发射均在此运行。
- **同步协调器**：登录或恢复会话后，`SyncManager` 为当前账户的每个同步组启动一个协调循环；本地 `notify` 事件经过 2 秒防抖，远端每 60 秒扫描一次。主窗口隐藏到托盘不影响循环；完全退出或退出登录时取消并落盘。
- **传输并发**：`TransferManager` 允许多个传输任务并行（每个任务一个 tokio task），并共享同一个 `reqwest::Client`（复用连接池与 rustls 会话）。

## 数据流

以“用户下载一个文件”为例：

```text
用户点击“下载”
  │
  ▼
[前端] FileList → download button
  │  services/koofr.ts::selectDownloadLocation()
  │  services/koofr.ts::prepareDownloadLocation()  ← Rust 校验父目录并签发一次性 auth ID
  │  services/koofr.ts::downloadFile()             ← 传 mountId、remotePath、authId
  │
  ▼
[Rust] commands::download_file
  │  1. 校验 mountId 与 remotePath（file_ops/）
  │  2. 消费 authId（LocalAccessManager，一次性、区分文件与文件夹）
  │  3. 创建 TransferManager 任务并返回 transferId
  │
  ▼
[Rust tokio task] transfer::download
  │  1. files/info → 获取 size / hash / modified
  │  2. 打开 .<name>.koofr-part-<transferId> 临时文件
  │  3. 写入 TransferCheckpointStore（transfer-checkpoints.json）
  │  4. 边下边写：reqwest stream → tokio::fs::File（每块 60 秒无数据视为网络停滞）
  │  5. 每 N 字节发送 "koofr://transfer-progress" 事件（不含文件名）
  │  6. 完成：校验大小与哈希，以不替换目标的方式发布到最终名称（名称被占用时追加序号）
  │  7. 删除 checkpoint
  │
  ▼
[前端] onTransferProgress 监听器 → 更新进度条
```

普通上传、分卷上传与文件夹下载采用类似模式，差异在于传输核心逻辑。

## 核心模块

### 前端 `src/`

目录职责见 [src/README.md](../src/README.md)。要点如下：

- `App.tsx` 负责全局状态协调（登录、弹窗、当前挂载点与目录等）；设置、传输与同步逻辑分别抽取到 `hooks/useAppSettings.ts`、`hooks/useTransferManager.ts`、`hooks/useSyncManager.ts`。技术栈为 React 19 + hooks，不使用 Redux / Zustand 等外部状态库。
- `features/*/` 每个功能模块只导出 UI 组件与领域 hooks。跨模块通信通过 `App.tsx` 传递 props 或事件回调，禁止 feature 之间直接依赖。
- 业务 Tauri 命令仅通过 `services/koofr.ts` 与 `services/publicLinks.ts` 的类型安全接口调用。当前 `components/TitleBar.tsx` 直接 import Tauri core / window API，仅用于执行原生窗口最小化、最大化与关闭；这是既有实现偏差，不能作为新增业务 IPC 绕过 services 的依据。
- 错误处理：使用 `commandErrorMessage(error, fallback)` 获取安全消息，使用 `isCommandErrorCode(error, code)` 判断错误码。

### Rust 后端 `src-tauri/src/`

```
src-tauri/src/
├── main.rs                     入口，几乎为空
├── lib.rs                      Tauri Builder + AppState 初始化
├── error.rs                    AppError + 稳定错误码 + 安全消息
├── commands.rs                 核心 Tauri 命令注册
├── folder_commands.rs          文件夹下载命令
├── link_commands.rs            分享链接命令
├── split_commands.rs           分卷上传命令
├── transfer_commands.rs        传输列表/恢复/丢弃命令
├── vault_commands.rs           Vault 注册、原生 Safe Key 与文件命令
├── stream_commands.rs          通用文件预览、文本读取与 Vault 内存解密
├── stream_session.rs           预览会话 + URI scheme handler 工具
├── preview_decode.rs           TIFF 与静态 LibRaw 内存解码
├── credential_manager.rs       Windows 凭据封装
├── settings.rs                 settings.json 读写
├── work_directory.rs           工作目录定位与启动期迁移
├── metadata_cache.rs           内存 + 磁盘缓存
├── local_access.rs             一次性本地路径授权
├── local_open.rs               打开下载文件/文件夹
├── folder_download.rs          递归文件夹下载
├── file_ops/                   路径规范化 + 操作校验
├── koofr_api/                  Koofr REST 客户端
├── transfer/
│   ├── mod.rs                  TransferManager + 公共类型
│   ├── manager.rs              状态管理
│   ├── model.rs                传输模型
│   ├── download.rs             单文件下载 + Range 续传
│   ├── upload.rs               普通整文件上传
│   ├── split_upload.rs         分卷可续传上传
│   ├── split_package.rs        分卷包元数据、清单、恢复脚本
│   ├── checkpoint.rs           TransferCheckpointStore（持久化）
│   ├── checkpoint_snapshot.rs  从磁盘 checkpoint 重建可恢复项
│   ├── part.rs                 .koofr-part-* 分片管理
│   └── vault.rs                Vault 加密上传 + 密文 Range 续传下载
├── crypto/                     koofr/vault 加密引擎边界
└── vault_core/                 解锁会话、不透明句柄、自动锁定
```

## 关键设计

### 认证与会话

- `connect_koofr(email, password, remember)` 将邮箱与 Koofr **应用专用密码**（不是主账户密码）POST 到 `https://app.koofr.net/token`。
- 成功后 Rust 内存保留单会话令牌（session token），**永不发送到前端**。
- `remember=true` 时，密码通过 `keyring-core` 与 `windows-native-keyring-store` 写入 Windows 凭据管理器，服务名为 `Koofr-GUI`，账号名为邮箱地址。
- 下次启动时 `restore_saved_login()` 从凭据管理器读回密码，并重新请求一次 `/token`。
- `disconnect_koofr()` 执行主动断开（含空闲自动登出）：将所有传输按“暂停”停止并最多等待 3 秒以便保存状态，停止同步循环，用 `zeroize` 清理会话令牌缓冲区，并清空 LocalAccessManager 中未消费的授权。可续传任务保留分片与 checkpoint，同一账户再次登录后可继续。`connect_koofr()` 切换账户时同样先暂停现有传输。
- 进程退出时同样执行 zeroize。

### 进程生命周期

- 应用通过 `tauri-plugin-single-instance` 以单实例运行。再次启动仅唤起已有主窗口，避免两个进程同时写入设置、检查点、历史与同步根。卸载时的 `--unregister-sync-roots` 清理入口在插件之前处理并直接退出。
- 主窗口关闭：启用“关闭到托盘”时仅隐藏窗口；否则执行与托盘“退出”相同的流程，不会只关闭窗口而留下后台进程。
- 退出流程：存在活动传输且启用了确认时先弹窗提示；随后暂停全部传输、停止同步、隐藏窗口并最多等待 3 秒让传输释放资源，再结束进程。
- 通过开机自启动（`--autostart` 参数）启动且启用了“关闭到托盘”时，启动后仅驻留托盘。

### Koofr Vault

- 加密实现固定使用官方 `koofr/vault` 的 `vault-crypto`，提交版本记录在 `Cargo.toml` / `Cargo.lock` 中；格式与 rclone crypt 兼容，不定义私有密文格式。
- Safe Key 通过 `CredUIPromptForCredentialsW` 的 Windows 原生凭据窗口直接进入 Rust。IPC、React 状态、日志、设置、缓存与传输检查点均不包含 Safe Key、salt 明文配置或派生密钥。
- scrypt 使用 rclone 默认参数（N=16384、r=8、p=1），派生出内容密钥、AES-EME 文件名密钥与 tweak。文件内容采用 `RCLONE\0\0` 文件头、24 字节随机 nonce、64 KiB 明文块与 XSalsa20-Poly1305 认证标签。
- 解锁后前端仅获得解密后的显示名、类型、明文大小与短期 UUID 句柄；密文路径、Vault validator 与 rclone 配置不离开 Rust。所有操作均由 Rust 将句柄重新解析到已注册 Vault 根目录内。
- 每个 Vault 独立锁定，默认在 60 分钟无 Vault 操作后自动清除会话。退出、重新登录或切换账户会立即清空全部解锁状态。正在执行的传输会持有完成当前传输所需的临时密钥引用。
- 加密上传流式生成带新 nonce 的 rclone 密文；由于 Koofr 仅支持整文件 `FilesPut`，恢复语义为从头安全重传。加密下载持久化的仅有密文远端路径与本地密文临时文件，使用 HTTP Range 字节续传；完整密文经逐块认证后写入独立明文临时文件，再以不覆盖方式发布。
- 创建时使用 v2 内容 validator，同时仍可读取旧版 EME 文件名 validator。导入 / 导出仅接受单个指向 `koofr:` 的 crypt 配置；导出文件使用 `create_new` 防止覆盖，其 `password` / `password2` 是 rclone 可恢复的混淆值，必须按敏感凭据处理。

### 文件预览

- `FilePreviewModal` 统一承载视频、音频、图片、PDF 与纯文本。支持格式由共享分类器决定；“我的文件”、最近文件、已共享与 Vault 可预览，回收站不进入预览流程。Office、OpenDocument、PSD、AI 与电子书仅保留下载。
- 普通媒体与 PDF 会话保存校验后的 mount / path / size / MIME，继续通过 `koofr-stream` 以最多 2 MiB 的 Range 分块读取。PDF.js 使用自定义 `PDFDataRangeTransport`，不会为了翻一页而完整下载普通 PDF。
- 文本仅读取前 5 MiB 并以 `<pre>` 纯文本呈现，不解析 HTML、Markdown、脚本或宏；后端支持自动检测以及 UTF-8、UTF-16、GB18030、Big5 手动选择。SVG 经 DOMPurify SVG profile 处理后再移除外部 URL，然后作为 Blob 图片显示。
- TIFF 在 Rust 内存中转换为 PNG；HEIC / HEIF 由本地 CSP worker 转换；RAW 由源码内固定的 LibRaw 0.21.1 静态库串行解码，优先使用最大内嵌预览，缺失时以半尺寸预览质量解码。普通完整解码输入不超过 100 MiB，RAW 输出不超过 80 MP。
- Vault 仅接受当前解锁会话签发的不透明 entry handle。Rust 重新解析密文路径，将单个不超过 50 MiB 的明文解密到 `Zeroizing<Vec<u8>>`，不写明文临时文件、缓存或历史；每次读取都会刷新 Vault 活动时间。关闭、锁定、自动锁定、账户切换、退出、30 分钟预览闲置或 LRU 淘汰都会移除并清零。
- 最多保留 32 个预览会话。日志仅包含随机 session ID、种类、字节数、解码器与稳定终态，不含文件名、路径、内容、Safe Key 或响应正文。

### 路径与操作校验

**远程路径**（Koofr 服务器上的路径）：

- 必须以 `/` 开头。
- 拒绝 `.`、`..`、空段、NUL 与超长段。
- 对根目录（`/`）的删除、重命名等破坏性操作直接拒绝。

**本地路径**：

- **上传**：只能通过 `select_upload_file()` 打开的原生文件对话框获得，前端拿到的是不透明句柄，不能自行拼接路径。
- **下载父目录**：用户可以手填，也可以使用 `select_download_folder()` 选择。Rust 会校验其为绝对路径、目录存在且不是符号链接。
- **最终下载文件名**：由 Rust 从远端名称清理生成（替换 Windows 非法字符、同级重名时追加序号），**前端无权指定**。
- 上传拒绝符号链接。

**授权模型**：`LocalAccessManager` 签发**一次性、区分文件与文件夹**的授权 ID。前端调用 `prepare_download_location()` 获取 ID，只能用一次，用于调用 `download_file()` 或 `download_folder()`。ID 使用后或进程重启即失效。

### 传输管线

`TransferManager` 维护一个 `HashMap<TransferId, TransferHandle>`，每个句柄包含：

- 当前状态：`Queued | Running | Retrying | Paused | Completed | Cancelled | Failed`
- 已传输字节数、总字节数
- 取消 signal（`tokio_util::sync::CancellationToken`）
- 关联的 checkpoint（如果可恢复）

进度通过 `koofr://transfer-progress` 事件推送。事件 payload：

```typescript
{
  transferId: string,
  direction: 'upload' | 'download',
  state: 'queued' | 'running' | 'retrying' | 'paused' | 'completed' | 'cancelled' | 'failed',
  bytesTransferred: number,
  totalBytes: number | null,
  splitProgress?: {
    completedParts: number,
    currentPart: number | null,
    totalParts: number,
    phase: 'parts' | 'finalizing',
  },
}
```

**payload 中不含文件名、路径或令牌**，前端自行维护 transferId 到文件名的映射。
分卷上传在每个 part 成功写入 checkpoint 后更新 `completedParts`；所有数据 part
完成后先进入 `finalizing` 阶段，直到整文件哈希、清单与恢复脚本上传完成。

### 单文件下载：字节级续传

`transfer::download` 的实现步骤：

1. 通过 `files/info` 获取远端大小、哈希与修改时间。
2. 选择目标名称时（`prepare_download_location`）跳过已存在的文件，以及仍在进行或已暂停的下载、已分配但尚未开始的授权将要使用的名称（按 Windows 大小写不敏感规则比较），必要时追加序号（`file (2).ext`）。
3. 打开 `.<name>.koofr-part-<transferId>` 临时文件（O_CREAT | O_RDWR）。
4. 将 `DownloadCheckpoint`（含 mountId、remotePath、expected_size、remote_hash、remote_modified、local_path、partial_path）写入 `transfer-checkpoints.json`。
5. 通过 `reqwest` 发起 `GET` 请求，将响应 body 流式写入临时文件。
6. **中断恢复**：应用重启后由 `list_resumable_transfers()` 从 checkpoint 重建可恢复项。用户点击“继续”时：
   - 再次请求 `files/info`，比对大小、哈希与修改时间。
   - 若一致：发送 `Range: bytes=<offset>-` 请求并追加写入。
   - 若服务端忽略 Range（返回 200 而非 206）：安全截断临时文件，从头重新下载。
   - 若服务端文件已变更（etag / modified 不匹配）：标记为 `Failed`，用户需重新开始下载。
7. 完成后以不替换目标的方式移动（Windows `MoveFileExW` 不带 `MOVEFILE_REPLACE_EXISTING`）发布到最终文件名；若下载期间该名称被占用，则改用下一个带序号的名称，并将实际路径写回下载历史。随后删除 checkpoint。

相关文件：`transfer/download.rs`、`transfer/part.rs`、`transfer/checkpoint.rs`。

### 分卷上传设计

**分卷的原因**：Koofr 公开上传 API 仅提供整文件 `PUT /content/api/v2/mounts/<mountId>/files/put`，不支持分块会话或服务端已确认偏移（可参考 rclone Koofr 后端与 Koofr 官方 Go 客户端）。因此普通上传一旦中断，只能重新传输整个文件。

分卷上传是针对该限制选定的互操作方案：

1. 用户在传输面板选择“可续传大文件”，并指定分卷大小（默认 100 MB，可配置）。
2. Rust 在远端创建用户命名的文件夹（例如 `MyBigFile.split/`）。
3. 本地将源文件流式切分为 `part-000.bin`、`part-001.bin` 等，**不包含专有文件头**，即原始字节切片。
4. 每个 part 上传成功后：
   - 计算 SHA-256。
   - 将 `SplitUploadCheckpoint`（含 `completed_chunks: Vec<SplitPart>`）写入 `transfer-checkpoints.json`。
5. 全部数据分卷完成后计算整文件 SHA-256，将结果写入检查点，再上传以下文件：
   - `SHA256SUMS`（各分卷与整文件 SHA-256）
   - `manifest.json`（分卷序号、大小、SHA-256 和原文件信息）
   - `README.txt`（中英双语，说明该文件夹用途与还原方式）
   - `restore.cmd`（Windows：`copy /b /y part-*.bin "<输出文件>"`）
   - `restore.sh`（POSIX：`cat part-*.bin > target.bin`）
6. 删除 checkpoint。

中断后，用户可在传输面板继续任务。应用重新核对远端分卷并补传缺失的分卷。若所有分卷已完成，则复用检查点中已保存的整文件 SHA-256，核对已存在的辅助文件内容并补传缺失文件。单个辅助文件的核对或上传超时按网络错误处理。

**远端可见性**：该文件夹在 Koofr 网页端或移动端呈现为普通文件夹，用户可自行下载并用 `copy /b` 或 `cat` 还原原文件，**不依赖本客户端**。这是有意的互操作保证。

相关文件：`transfer/split_upload.rs`、`transfer/split_package.rs`、`split_commands.rs`。

### 文件夹递归下载

`folder_download.rs` 的实现步骤：

1. 通过 Koofr API 递归列出目标文件夹的全部子项，构建清单。
2. 在下载父目录下创建**临时目录** `.<name>.koofr-part-<随机 UUID>/`，用于暂存所有内容。
3. 逐个文件流式写入暂存目录（过程中可取消）。
4. 全部完成后以不替换目标的方式将暂存目录发布为最终目录名（名称被占用时追加序号）。
5. **失败或取消时清理整个临时目录**，不留半成品；清理本身失败时仍报告原始错误。

文件夹下载**不做**磁盘 checkpoint（重启后无法续传整个文件夹），因为 Koofr 目录清单可能已变化；从头重新开始比“部分恢复导致状态混淆”更安全。暂停文件夹下载等同于停止，下载历史中此类“已暂停”记录会在下次校正时转为失败，可以清除。

### 网络错误自动恢复

设置中的 `autoRetryNetworkErrors` 仅匹配网络类错误（`AppError::is_network()`：`AppError::Network` 与 `AppError::NetworkStalled`，前端错误码均为 `network_error`）。`networkRetryLimit` 表示初次请求失败后的最大重试次数，`null` 表示无限；`networkRetryIntervalSeconds` 为每次重试前的固定等待时间。等待期间会发出 `Retrying` 状态，暂停与取消仍由同一个 `CancellationToken` 控制。

- 单文件下载重新读取临时文件长度，并使用 HTTP Range 从已落盘偏移继续。
- 分卷上传重新核对远端完整分卷，从最后一个已确认分卷继续。
- 普通上传受 Koofr 整文件 `FilesPut` 接口限制，每次重试必须从文件开头重新发送。
- 文件夹下载会清理本次失败的暂存树，然后从头重建目录下载。
- 权限、冲突、本地 I/O、HTTP 状态与内容完整性错误不会触发该策略。
- 每次调度重试都会写入脱敏日志事件 `network_retry_scheduled`，仅包含传输 ID、方向、次数与延迟。

超时边界（避免停滞的连接使调用方永久等待）：

- 元数据请求（列目录、`files/info`、重命名、删除、登录等）整体超时为 60 秒。
- 内容请求（上传 / 下载 / 预览）没有整体超时；下载与预览等待响应头最多 60 秒，响应体相邻两块之间超过 60 秒无数据即返回 `NetworkStalled`。
- 普通、Vault 与分卷上传以及同步上传在 5 分钟内无任何上传进度时返回 `NetworkStalled`。
- 客户端启用 30 秒 TCP keepalive。按需水合保留自身的 60 秒无进展超时。

### 应用工作目录

默认工作目录为 Tauri `app_local_data_dir`。用户可在设置中选择另一个已有的空目录，
并决定是否将当前工作目录中的设置、传输检查点、下载历史、缓存、日志及其他文件全量
迁移过去。变更不会在运行中搬动已打开的文件，而是写入默认数据目录旁的
`net.koofr.desktop.gui.work-directory.json` 定位记录，并在下次启动、构造 `AppState` 之前执行迁移。

全量迁移先递归复制到目标同级的应用专用暂存目录，拒绝符号链接与特殊文件；写入迁移
标记后再激活目标，最后清理旧目录。该设计既支持跨卷移动，也能在进程或 I/O 中断后区分
源目录、完整目标与暂存副本。只有目标激活后才清理源文件；失败时保留可用副本并在下次
启动时重试。目标必须为空且不是文件系统根目录，新旧目录不得相同或互相嵌套。

该定位记录是定位自定义目录所必需的引导数据，不属于可迁移的工作数据；默认工作目录
作为源时，该记录会留在原位。Windows Credential Manager 中的应用专用密码同样不属于
工作目录，既不会经 IPC 暴露，也不会被迁移。

### 元数据缓存

`MetadataCache` 提供三种模式：

| 模式 | 存储位置 | TTL |
| --- | --- | --- |
| `Off` | 无缓存，每次请求 Koofr | - |
| `Memory` | 进程内存 `RwLock<HashMap>` | 可配置（默认 15 分钟）|
| `Disk` | 当前工作目录的 `cache/metadata-cache.json` | 可配置 |

- 缓存 key 为 `(userId, mountId, remotePath)`，按账户完全隔离。
- 缓存 value **仅包含**文件名、远程路径、大小与修改时间，**不缓存令牌、密码或文件内容**。
- 切换模式、清缓存或切换账户时自动失效。
- 磁盘缓存随整个工作目录迁移，不再单独配置路径。

### 诊断日志

`AppLogger` 使用独立后台线程写入当前工作目录的 `logs/`，不阻塞 Tauri 命令或传输任务。设置页可指定记录级别、保留天数与单文件大小上限，也可查看占用情况并一键清理。

- 活动文件为 `koofr-gui.jsonl`，达到大小上限后按带时间戳与随机 ID 的文件名轮转。
- 轮转文件按保留天数自动清理；默认保留 14 天，单文件上限 10 MB。
- 常规传输日志记录 session ID、transfer ID、方向、终态、字节数、稳定错误码，以及脱敏后的 HTTP 状态、I/O 类别与网络失败分类。
- 调试级别使用 `operation_started` 和 `operation_finished` 记录阶段、耗时和结果。用同一 session ID 内的 `fields.operationId` 配对。网络阶段区分收到 HTTP 响应头与读取、解析 JSON 响应。网络标签使用固定操作名。
- 分卷日志记录远端分卷核对、分卷提交、收尾、整文件校验值保存，以及各类辅助文件的验证或上传。`split_hash_progress` 每 10 秒记录已完成哈希的字节数，可区分本地校验与后续 Koofr 请求。
- 调试日志还覆盖普通上传排队与重试、下载续传偏移与校验发布、文件夹下载、同步扫描周期与 CFAPI 回调、凭据读取、Vault 解锁与浏览以及预览下载与解码。日志阶段使用固定标签，辅助文件使用用途标签。
- **不记录**认证头、令牌、密码、邮箱、文件名、完整本地路径、远程路径或服务端响应正文。

### 凭据存储

- **会话令牌**：仅存于 Rust 进程内存，使用 `zeroize` 清理。
- **应用专用密码**（用户勾选“记住我”时）：Windows 凭据管理器，服务名为 `Koofr-GUI`，账号名为邮箱地址。
- **服务端返回的错误响应正文**：读取一次取得错误码后立即丢弃，不写入日志。

认证失败或用户勾选“忘记我”时，从 Credential Manager 删除对应记录。

### 错误模型

`src-tauri/src/error.rs`:

```rust
pub enum AppError {
    Network,
    Unauthorized,
    NotFound,
    Conflict,
    InvalidInput(&'static str),
    ServerError,
    LocalData,
    LocalIo,
    Cancelled,
    // ...
}
```

序列化给前端时仅暴露：

```typescript
{ code: string, message: string }
```

- `code` 为稳定的英文标识（例如 `unauthorized`、`not_found`、`invalid_input:remote_path`），前端可用于逻辑判断（`isCommandErrorCode`）。
- `message` 为安全的国际化消息，**不含**具体路径、令牌或服务端响应正文。

## 设置驱动的运行时行为

- `settings.json` 在 Tauri setup 阶段加载，React 启动时也会与登录恢复并行读取，因此语言、密度、动效、并发、限速、通知与空闲登出不依赖打开设置面板。
- `TransferManager` 先注册任务，再通过上传 / 下载动态闸门进入 `queued` 或 `running` 状态。降低并发仅阻止新任务进入，提高并发会立即唤醒队列；排队任务共用同一个取消令牌。
- `BandwidthLimiter` 是普通、分卷、文件夹与 Vault 文件传输共享的 token bucket。全局与单任务限速均以 KiB/s 计算，`0` 表示该层不限速，容量允许 1 秒突发；媒体预览与元数据请求不经过限速器。
- 下载在临时文件 `sync_all` 之后、改名发布之前校验 Koofr MD5。缺少合法 hash 时记录 `unavailable`，不一致则返回 `integrity_error`。Vault 在远端密文校验后仍执行加密认证。
- 失败的临时文件由 checkpoint 记录精确路径与保留时间。到期清理会先重新生成 checkpoint snapshot 验证路径，仅删除该精确普通文件并同步移除 checkpoint，不扫描下载目录。
- 托盘、开机启动、通知与传输悬浮窗使用 Tauri 官方能力。悬浮窗默认关闭，整个表面可用左键拖动；右键菜单由校验窗口标签的 Rust 命令弹出。关闭菜单会同步关闭持久设置，安全退出会复用活动传输确认。首次提示状态与拖动后的物理坐标存入 `settings.json`；显示器布局变化时坐标会被收回到可见范围。外部项目页面仅能通过 Rust 固定枚举打开；WebView 没有任意 shell 或 URL 权限。

## 本地文件夹同步

同步范围限定为 Koofr 主存储中的普通目录；Rust 侧会再次拒绝 Vault、共享或第三方挂载、非法路径，以及本地或远端根目录重叠。每组配置只保存经原生文件夹选择器一次性授权后得到的绝对目录，并在每次扫描前重新检查目录存在、不是符号链接且仍为目录。

`sync-state.sqlite3` 使用 WAL 与事务保存按账户隔离的同步组、三方基线、问题及操作状态。首次同步仅合并，不执行删除；之后仅在基线仍匹配时传播删除，远端使用 Koofr 条件删除进入回收站，本地使用 Windows 回收站，失败时保留文件。双侧修改保留远端原名，并给本地版本追加带 UTC 时间的冲突名；任何条件上传或删除冲突都会触发重新扫描，而非强制覆盖。一轮成功后，新基线只记录两端一致的条目（目录同为目录、文件内容相同）；同步期间发生分歧的条目保留上一轮基线，留待下一轮处理。仅当某目录曾在基线中出现于两端时才传播删除，基线中缺少任一侧的目录一律视为新建；本地目录删除在子项处理完成后仅移除空目录（经典同步与按需同步一致），若仍有本地内容则改为恢复远端目录。下载在替换本地文件前按大小与修改时间再次核对计划时的本地状态，不一致即转为冲突。条件冲突会立即触发重扫（连续最多 3 次，之后回到 60 秒周期）；部分操作失败时仍保存两端一致部分的基线并保留问题列表，同一路径与错误码只保留一条未解决问题；任何非取消的运行失败都会把同步组置为离线（网络或登录失效）或需要处理，并同步 Provider 状态。

本地扫描跳过符号链接、重解析点、特殊文件、系统回收站目录与客户端临时项；普通隐藏文件参与同步。根目录 `.syncignore` 支持空行、注释、gitignore 风格 glob 与 `!` 反向包含，规则变化会通过监听触发完整重扫。同步操作复用传输并发闸门、限速、取消与网络错误重试，但单独写入同步操作日志，不进入手动传输恢复列表。

Windows 按需同步作为预览模式与经典同步并存。`cloud_files.rs` 使用固定 Provider GUID 注册部分填充同步根，目录首次展开通过 `FETCH_PLACEHOLDERS` 仅列出当前层，文件打开通过 `FETCH_DATA` 完整水合。回调线程仅复制路径、随机条目键与请求范围，再将工作投递到异步运行时；网络等待以 60 秒无进展超时保护，写入 `CfExecute` 的中间块按 4 KiB 对齐。续传时核对响应 `ETag`（无则 `Last-Modified`），版本变化即中止；由于数据已逐块交给 CFAPI，MD5 与长度校验只能在水合末尾报告失败，无法回滚已写入的块。

占位符身份格式为 `KFG1:<UUID>`，不含账户、文件名或远端路径。`sync_entries` 表按账户所属同步组保存 UUID 到相对路径、远端路径、类型、大小、修改时间与 MD5 的映射。原生扫描先查询 CFAPI 状态：已同步的占位符直接复用数据库身份且不打开或哈希；只有本地新增、修改或尚未转换的完整文件才读取内容。远端轮询仅递归根目录与已填充的目录，未访问目录在 Explorer 首次展开时读取最新内容。

按需根采用完整文件水合、自动释放空间、禁止硬链接与仅 Provider 管理占位符的策略。水合复用全局下载并发、限速、取消与 MD5 校验；本地重命名在身份唯一匹配时优先调用 Koofr `move`。资源管理器删除占位符会进入异步 `NOTIFY_DELETE` 处理：Provider 先校验同步组、回调路径与条目身份，并立即通过 `ACK_DELETE` 结束 Windows 删除请求，再异步执行 Koofr 条件删除；远端请求失败时由三方基线扫描重试或进入冲突协调，不能让资源管理器进度框等待网络。暂停、扫描、同步、离线与错误会同时更新应用同步事件和 Windows Provider 状态。

经典迁移要求旧组处于成功空闲状态且无问题，并选择新的空目录。旧组暂停后，新根以 `staging` 注册并完成首次元数据与占位符验证；只有达到成功空闲才标为 `ready` 并移除旧配置。60 秒超时或任意失败会解除暂存根并恢复旧组，旧目录从不移动或删除。解除注册前必须断开连接并核对 Provider 名称与 `KFG1:<pairId>` 根身份；NSIS `PREUNINSTALL` 在非升级卸载时调用同一无界面清理入口，失败时会要求重试或取消。

## 事件与命令目录

**Tauri 命令**（前端 → Rust，由各 `*_commands.rs` 注册）：

认证 / 会话：`connect_koofr`, `restore_saved_login`, `disconnect_koofr`, `koofr_session`, `forget_saved_login`

设置：`get_settings`, `update_settings`, `update_download_settings`, `update_logging_settings`, `update_transfer_settings`, `update_work_directory`, `clear_metadata_cache`, `clear_logs`, `select_work_directory`, `update_appearance_settings`, `update_general_settings`, `update_transfer_advanced_settings`, `update_download_behavior_settings`, `update_bandwidth_settings`, `update_notification_settings`, `update_link_defaults`, `update_security_settings`, `update_history_policy`, `app_version_info`

文件浏览：`list_mounts`, `list_files`, `list_recent`, `list_shared`, `list_trash`

文件操作：`create_folder`, `rename_entry`, `move_entry`, `copy_entry`, `delete_entry`, `restore_trash`, `empty_trash`, `open_downloaded_file`, `open_downloaded_folder`, `open_external_page`

本地路径授权：`select_upload_file`, `select_download_location`, `select_download_folder`, `select_download_directory`, `prepare_download_location`, `prepare_download_folder`

同步：`select_sync_folder`, `list_sync_pairs`, `create_sync_pair`, `get_native_sync_capability`, `suggested_on_demand_sync_root`, `create_on_demand_sync_pair`, `migrate_sync_pair_to_on_demand`, `open_sync_root`, `set_sync_pair_paused`, `run_sync_pair_now`, `list_sync_issues`, `list_sync_tasks`, `remove_sync_pair`

传输：`upload_file`, `upload_split_file`, `download_file`, `download_folder`, `cancel_transfer`, `pause_transfer`, `list_resumable_transfers`, `list_download_history`, `clear_finished_download_history`, `resume_transfer`, `discard_resumable_transfer`, `show_floating_context_menu`, `set_floating_window_compact`, `open_main_window_from_floating`, `set_taskbar_badge`

分享链接：`list_public_links`, `create_public_link`, `delete_public_link`

Vault：`list_vaults`, `unlock_vault`, `lock_vault`, `list_vault_files`, `create_vault_folder`, `rename_vault_entry`, `relocate_vault_entry`, `delete_vault_entries`, `upload_vault_file`, `download_vault_file`, `create_vault`, `remove_vault`, `export_vault_rclone_config`, `import_vault_rclone_config`

文件预览：`prepare_file_preview`, `prepare_vault_preview`, `read_preview_text`, `close_file_preview`

**自定义 URI scheme**（Rust → WebView）：

- `koofr-stream://`（Windows：`http://koofr-stream.localhost/{sessionId}`）：通用预览 Range 代理。Rust handler 从 `StreamSessionManager` 读取校验后的远端源或 Vault 内存源，返回不超过 2 MiB 的 `206 + Content-Range`。CORS 仅允许 Tauri 生产 origin 与固定的 `127.0.0.1:1420` / `localhost:1420` 开发 origin；令牌、密文路径与 Safe Key 不会暴露给 WebView。

**Tauri 事件**（Rust → 前端）：

- `koofr://transfer-progress`：传输进度与状态变化。
- `koofr://floating-transfer-state`：主窗口 → `transfer-float` 的脱敏聚合状态；仅含方向、任务数、百分比、外观与交互偏好、可恢复暂停数与完成计数。
- `koofr://floating-transfer-action`：`transfer-float` → 主窗口的脱敏批量暂停 / 继续意图，不含任务标识。
- `koofr://floating-window-disabled`：原生右键菜单关闭悬浮窗后，同步主窗口中的通知设置开关。
- `koofr://sync-state`：同步组级状态、待处理与已完成项目及字节汇总、最近成功时间、问题数量与稳定错误码；不含文件名、完整路径或服务端响应正文。

## 本地数据布局

上述 JSON 文件（设置、传输检查点、下载历史、元数据缓存、工作目录定位记录）每次写入均使用独立的 `.<uuid>.tmp` 临时文件，`sync_all` 后一次性替换目标；同一存储的写入由互斥锁串行化。读取时若文件无法解析或版本未知，会被改名为 `<原名>.corrupt-<毫秒时间戳>` 保留，并在诊断日志记录 `state_file_quarantined`（仅含存储类别），随后以默认值继续。

```text
<应用工作目录>\
├─ settings.json                # AppSettings（下载、缓存与日志策略等）
├─ transfer-checkpoints.json    # TransferCheckpointStore（可恢复任务）
├─ download-history.json        # 按账户隔离的下载历史、目标位置与限量速度采样
├─ sync-state.sqlite3           # 同步组、占位符条目、设备标识、双边基线、问题与操作日志（WAL）
├─ cache/metadata-cache.json    # 磁盘缓存
└─ logs/koofr-gui*.jsonl        # 诊断日志
```

默认位置旁另保留 `%LOCALAPPDATA%\net.koofr.desktop.gui.work-directory.json` 定位
记录；使用自定义工作目录时，该文件仍留在原位。

**Windows 凭据管理器**（不位于上述目录）：

- 服务名 `Koofr-GUI`，账号名 `<email>`：用户勾选“记住我”时保存的应用专用密码。

**下载临时文件**（位于用户配置的下载目录下）：

- `.<name>.koofr-part-<transferId>`：普通单文件下载暂存；Vault 下载时保存完整密文，同样按传输 ID 隔离。
- `.<name>.koofr-vault-plain-<transferId>`：Vault 完整认证后的短期明文暂存；成功后以不替换方式发布为最终文件名，再删除密文分片；失败时清理。
- `.<name>.koofr-part-<随机 UUID>/`：文件夹下载暂存目录。

清理或卸载应用时，上述三处均不会自动删除，需要用户手动处理。

## 未实现的部分

### OAuth 与第三方存储管理

需要 Koofr 提供公版桌面客户端注册信息（client_id）与公开授权 API。在此之前：

- 应用可以列出账户中已连接的 Google Drive、OneDrive、Dropbox 等存储，并正常读写。
- 新增、移除或重新授权只能引导用户前往 [Koofr 官方账户页面](https://app.koofr.net/app/admin/connections) 操作。

### 平台

macOS 与 Linux 的支持排期尚未确定。Tauri 本身跨平台，主要障碍包括：Windows 凭据管理器需替换为 `keyring` 后端；打包与分发流程需重做；对应平台的原生文件对话框行为差异需要测试。
