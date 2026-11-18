# Rust / Tauri 后端

后端实现 Koofr 文件管理、传输控制、凭据管理、本地缓存与加密工作区等核心功能：

- `src/file_ops/`：校验挂载点 ID、远程路径、远程名称与本地选择路径。
- `src/koofr_api/`：认证、挂载点、目录列表、创建文件夹、复制、移动、删除、内容请求与基础分享链接。
- `src/link_commands.rs`：下载链接与接收文件链接的查询、创建、撤销。
- `src/transfer/`、`src/folder_download.rs`：流式上传与下载、单文件 Range 续传、通用二进制分卷续传、持久化恢复检查点、递归目录清单、进度事件、取消与下载临时内容清理。
- `src/sync.rs`：按账户隔离的经典同步与按需同步；SQLite WAL 基线、占位符条目、问题与操作日志、本地监听、远端扫描、`.syncignore`、三方比较、条件上传与删除，以及回收站删除。
- `src/cloud_files.rs`：Windows CFAPI 能力检测、同步根注册与连接、占位符创建与更新、数据水合、固定状态、Provider 状态，以及校验身份后的解除注册。
- `src/transfer/manager.rs`、`src/transfer/limiter.rs`：可动态升降的上传 / 下载闸门、排队取消，以及覆盖普通、分卷、文件夹、Vault 与恢复任务的共享 KiB/s token bucket。
- `src/transfer/integrity.rs`：最终发布前的流式 MD5 校验；缺少远端 hash 时记为不可校验，hash 不一致时不发布最终文件。
- `src/desktop.rs`、`src/autostart.rs`：官方 Tauri 托盘与 autostart 生命周期、活动传输退出确认、Windows 任务栏 overlay，以及置顶传输悬浮窗的位置恢复、可中断收展、原生右键菜单与主窗口唤起。
- `src/commands.rs`、`src/vault_commands.rs`：暴露给 WebView 的窄接口；Vault 命令仅接收注册 ID 与短期不透明句柄。
- `src/stream_commands.rs`、`src/stream_session.rs`、`src/preview_decode.rs`：通用文件预览会话、受限 Range 代理、文本编码与 TIFF / RAW 内存解码。
- `src/crypto/`、`src/vault_core/`：官方 `koofr/vault` 加密引擎边界、Vault 解锁会话、不透明路径句柄与自动锁定。
- `src/credential_manager.rs`：用户选择保存密码时，将应用专用密码写入 Windows 凭据管理器（服务名 `Koofr-GUI`，账号名为邮箱地址）。密码不会出现在普通配置文件中。
- `src/settings.rs`、`src/metadata_cache.rs`：保存非敏感设置，并为挂载点、目录、最近文件、共享与回收站提供按账户隔离的 TTL 缓存。
- `src/work_directory.rs`：保存工作目录定位记录，并在启动早期执行可恢复的全量目录迁移。
- `src/logging.rs`：后台写入结构化 JSONL 诊断日志，支持级别过滤、大小轮转、保留期限与清理。

## 会话与安全边界

以下约束均有明确的实现依据，修改相关代码前请逐条确认：

- `connect_koofr` 将邮箱与 Koofr 应用专用密码发送至 `https://app.koofr.net/token`，仅保留返回的会话令牌。断开连接或进程退出时会清零令牌缓冲区。
- 重新连接或断开账户会取消当前传输，并清除尚未使用的本地文件授权。
- 登录或恢复会话后会启动当前账户的同步组；退出登录、切换账户或进程完全退出时取消同步循环。关闭到托盘仅隐藏窗口，同步继续运行；未启用关闭到托盘时，主窗口关闭按钮会退出整个应用。应用通过 `tauri-plugin-single-instance` 以单实例运行。
- 退出应用、退出登录与切换账户时，手动传输按“暂停”停止（`TransferManager::pause_all`），并最多等待 3 秒让各任务保存状态；可续传任务保留分片与检查点。
- 远程路径必须是规范的绝对 Koofr 路径。`.`、`..`、空段、NUL、超长名称以及对根目录的破坏性操作全部拒绝。
- 上传路径只能由 Rust 打开的原生文件对话框授权。下载父目录可由用户手填，也可通过原生文件夹选择器选择；Rust 会验证其为现有绝对目录且不是符号链接，仅在该目录下使用清理后的远端名称创建新目标，并签发一次性、区分文件与文件夹的授权 ID。前端不能指定最终文件名，也不能覆盖已有内容。上传同样拒绝符号链接。
- 下载不覆盖现有文件。单文件先写入由传输 ID 确定的 `.koofr-part-*` 临时文件。不含凭据的恢复元数据保存到当前 Windows 用户的应用数据目录，并按 Koofr 用户 ID 隔离账户（服务缺少该字段时回退为邮箱指纹）。网络中断、退出或暂停后，Rust 会重新核对远端大小、修改时间与可用哈希，然后发送 `Range` 请求，从已落盘偏移继续。若服务端忽略 Range，则安全截断分片并从头下载。文件夹下载仍使用临时目录，失败或取消时清理整个暂存树。Windows 非法名称会被安全替换，同级清理后重名会稳定追加序号；选择名称时还会避开未完成下载与待用授权的目标，最终发布使用不替换目标的移动，名称被占用时换用下一个序号，并将实际路径写入下载历史。
- 返回给前端的错误仅包含稳定错误码与安全消息，不包含令牌、本地路径、远程路径或服务端响应正文。
- 诊断日志涵盖请求和本地操作的阶段、耗时、字节数与脱敏错误。调试事件使用固定操作名和随机会话内的操作编号。各级别均排除凭据、邮箱、文件名、路径、请求参数及响应正文。
- Vault Safe Key 仅由 Windows 原生凭据窗口交给 Rust，不经过 IPC。前端只能看到解密后的显示元数据与 UUID 句柄；密文路径、validator 与 rclone 配置均留在后端。默认在 60 分钟无操作后自动锁定，账户切换与退出会立即清空解锁状态。
- Vault 预览先用不透明句柄重新解析目标，明文仅保存在 `Zeroizing` 内存且不超过 50 MiB；锁定、自动锁定、关闭预览、切换账户或退出时立即清除。普通完整图片与 RAW 解码输入不超过 100 MiB，RAW 输出限制为 80 MP 并串行执行。

会话令牌仅存于内存。用户勾选“保存密码”后，应用专用密码由 Windows 凭据管理器（服务名 `Koofr-GUI`，账号名为邮箱地址）保护，下次启动时由 Rust 后端读取并重新认证。当前认证仅支持使用应用专用密码进行直接会话初始化。

文件元数据缓存默认仅存在于内存。用户可在设置中启用磁盘缓存，文件固定放在当前工作目录的 `cache/` 中；缓存仅包含普通 Koofr 文件名与远程路径，不包含密码、令牌或文件内容。切换到“不缓存”、清除缓存或切换账户时，缓存条目会自动删除。

工作目录默认为 Tauri 的 `app_local_data_dir`，也可切换到用户选择的空目录。变更会先写入默认数据目录旁的 `net.koofr.desktop.gui.work-directory.json` 定位记录，下次启动时在设置、检查点、历史、缓存与日志初始化之前完成迁移。全量迁移使用独立暂存目录与迁移标记，成功激活新目录后才清理旧数据；失败时保留可用副本，并在后续启动时重试。根目录、符号链接、非空目标以及互相嵌套的新旧目录均会被拒绝。Windows 凭据管理器中的应用专用密码不属于工作目录。

用户可选择启用 `network_error` 自动恢复，并设置有限或无限的重试次数以及固定重试间隔。单文件下载从已落盘偏移继续，分卷上传从已确认分卷继续，普通上传因 Koofr 仅提供整文件上传接口而从头重试。等待期间仍接受暂停与取消，其他错误类别不会自动重试。元数据请求整体超时 60 秒；下载与预览的响应头或相邻数据块间隔超过 60 秒、上传 5 分钟无进度时返回 `AppError::NetworkStalled`，前端错误码同为 `network_error`，同样参与自动重试。

## Tauri 命令

后端注册的完整 Tauri 命令清单与自定义 URI scheme 规范，请参阅[架构说明中的事件与命令目录](../docs/ARCHITECTURE.md#事件与命令目录)。

传输通过 `koofr://transfer-progress` 事件上报运行、暂停、完成、取消或失败状态；分卷任务额外携带已确认卷数、当前卷、总卷数与 `parts` / `finalizing` 阶段。事件中不包含本地或远程文件名。主窗口另向 `transfer-float` 发送仅含方向、任务数与汇总百分比的 `koofr://floating-transfer-state`。TypeScript 封装位于 `src/services/koofr.ts`。

同步通过 `koofr://sync-state` 推送组级状态、模式与根类型、原生连接状态、在线 / 已下载 / 固定数量、待处理与已完成项目及字节汇总、最近成功时间、问题数与稳定错误码；文件级问题的相对路径仅作为结构字段返回，不写入日志或错误正文。同步传输复用并发闸门、限速、取消与网络重试，但不进入手动传输恢复列表。

Koofr 官方 Go 客户端提供 `FilesGetRange`，因此下载可实现真正的字节级续传。但公开上传协议与 rclone 的 Koofr 后端仅支持整文件 `FilesPut`，没有针对单个普通文件的分块上传会话或已确认偏移。普通上传会持久化中断任务并提供“重新上传”，不会将整文件重传描述为字节续传。用户明确选择“可续传大文件”时，后端会创建独立的远端文件夹，将原文件切分为自定义大小的 `part-*.bin`，并仅从最后一个已确认完整的分卷继续。完成后写入通用恢复命令、分卷与整文件 SHA-256，以及开放 JSON 清单。分卷可直接使用系统自带的 `copy /b` 或 `cat` 拼接，无需本客户端。

分卷收尾前校验源文件大小和修改时间，并保存整文件 SHA-256。恢复时复用此校验值，确认已存在的辅助文件内容一致后补传缺少的文件。单个辅助文件的校验或上传最多等待 60 秒，收尾期间可以暂停或取消。

Vault 文件名使用 AES-EME，内容使用 rclone crypt 的认证分块格式，密钥派生参数与 Koofr Vault / rclone 保持一致。加密上传受 `FilesPut` 限制只能整文件安全重试；下载先对密文做 HTTP Range 续传，完整逐块认证与解密后才发布最终明文。互操作 golden vector 来自 rclone 与官方 `koofr/vault`。

## 检查

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

根目录的 `npm run check` 会连前端检查一起跑这些命令。

## 参考实现

请求路径和载荷以 Koofr 官方 [Go 客户端](https://github.com/koofr/go-koofrclient)、[Java SDK](https://github.com/koofr/java-koofr) 和 [Koofr Vault](https://github.com/koofr/vault) 为准。命令和原生文件选择遵循 [Tauri v2 命令文档](https://v2.tauri.app/develop/calling-rust/) 和 [Dialog 插件文档](https://v2.tauri.app/plugin/dialog/)。
