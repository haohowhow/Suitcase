# 更新日志

格式按 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，版本号按 [SemVer](https://semver.org/spec/v2.0.0.html)。

## [Unreleased]

### Changed
- 对项目整体文档（包括 README、构建与发布指南、安全策略、贡献指南及第三方声明等）进行全面整理与专业化修正，确保技术参数、安全边界与代码实现保持一致。

## [2.3.1] - 2026-07-30

### Added
- 传输悬浮窗在暂停或空闲时可收为单图标圆角方块；活动时单击可暂停全部活动任务、暂停时单击可继续全部可恢复任务，双击打开应用。通知设置可分别关闭自动收起、单击切换和双击打开；窗口收展遵循减少动效偏好。
- 将视频 / 音频播放器扩展为统一文件预览器，覆盖“我的文件”、最近文件、已共享和 Vault；支持 PDF 分页缩放、文本编码切换与截断提示、常用图片、SVG 净化、TIFF / HEIC / RAW 本地解码，以及图片缩放、适应窗口和旋转
- 支持格式双击改为预览，行尾及单选工具栏同时保留“预览”和“下载”；回收站与 Office、OpenDocument、PSD、AI、电子书仍保持下载后查看
- 固定并随源码分发 LibRaw 0.21.1，以 CDDL-1.0 选项静态构建；PDF.js、DOMPurify 和 HEIC 解码器全部锁定并离线打包

### Fixed
- PDF preview Range requests use 2 MiB chunks, preventing large PDFs from stalling after page-count discovery.

## [2.2.2] - 2026-07-28

### Added
- Added unified local previews for PDFs, text, images, SVG, media, and supported RAW formats across file lists, recent files, shared files, and Vault.
- Bundled PDF.js assets, HEIC support, and static LibRaw resources for offline desktop builds.

### Fixed
- PDF preview Range requests now use 2 MiB chunks instead of PDF.js's 64 KiB default, preventing large PDFs from stalling after page-count discovery.
- PDF CMaps, standard fonts, CSP-safe image decoders, and React StrictMode lifecycle handling are now included and stable in release WebView2 builds.

### Security
- Preview decoding remains local; text/HTML is rendered as plain text and SVG content is sanitized before display.

## [2.2.1] - 2026-07-27

### Fixed
- 悬浮窗左键单击现在打开主应用面板，不再只能拖拽或右键菜单打开；拖拽需鼠标移动 4px 以上才触发
- 外观设置中「界面密度」与「减少动效」之间增加间距和分隔线，避免视觉过于紧凑

### Changed
- 外观设置新增「显示传输悬浮窗」开关，通知设置中的原开关也保留

### Security
- 新增 `open_main_window_from_floating` 命令有窗口标签校验，非悬浮窗窗口无法调用

## [2.2.0] - 2026-07-27

### Added
- 分卷上传在传输列表和详情中显示当前 / 总分卷、已确认分卷以及辅助清单生成阶段，恢复任务也能还原该信息
- 新增可选的 Windows 置顶传输悬浮窗：按字节聚合并发上传 / 下载进度，区分暂停、完成和空闲状态；整个圆角表面可左键拖动，右键菜单可打开应用、关闭悬浮窗或安全退出
- 首次进入支持悬浮窗的版本时询问一次是否启用，通知设置中可随时开关

### Security
- 悬浮窗使用独立最小 capability，状态快照只含传输方向、任务数和汇总百分比，不发送文件名、路径、账户或令牌

## [2.1.0] - 2026-07-26

### Added
- 设置面板重构为左侧分组导航 + 右侧内容布局，覆盖 11 个分区：外观、通用、下载、传输、缓存、诊断日志、分享链接默认值、通知、隐私与安全、下载历史、关于
- 设置在应用启动时即与登录恢复并行加载；简中、英文和跟随系统可即时切换，紧凑密度与减少动态效果全局生效
- 使用 Tauri 官方 autostart、notification 和 tray 能力，实现事务化开机启动、常驻托盘、关闭/最小化到托盘、托盘恢复、传输退出确认和原生完成通知
- 新增动态上传/下载并发闸门与 `queued` 状态，覆盖普通、分卷、文件夹、Vault 及恢复任务；排队任务支持暂停和取消
- 新增动态共享 token-bucket：支持全局与单任务 KiB/s 限制、1 秒突发，以及 Windows 连续五分钟无输入后全速
- 下载发布前按 Koofr MD5 校验普通、文件夹和 Vault 密文，传输结果与历史记录显示 `notRequested` / `unavailable` / `verified`
- 普通上传超过阈值时先询问是否改用分卷，分卷对话框采用设置中的默认 part 大小
- 分享链接默认类型、有效期和随机密码已接入 Koofr `/validity` 与 `/password/reset`；随机密码只在当前创建响应中显示
- 任务栏 overlay 角标提供 `1`–`9` 与 `9+`，活动任务清零或关闭角标时自动移除
- 下载历史按账户即时应用保留天数与条数；失败临时文件只按经验证 checkpoint 的精确路径到期清理
- 文件列表补充“移动”按钮，支持选择目标目录并安全移动文件或文件夹

### Changed
- 删除下载“覆盖已有文件”策略；旧 `overwrite` 配置自动迁移为 `rename`，任何下载仍不覆盖现有文件
- 删除没有业务入口的诊断路径剪贴板开关；日志继续始终强制脱敏
- 关于页外部入口改由 Rust 固定 URL 白名单打开，不向 WebView 开放任意 URL 或 shell 权限
- 工作目录迁移仍按既有安全设计在下次启动、任何存储初始化前执行

### Security
- 通知和 overlay 只使用前端已有显示名与任务数，不包含完整路径、令牌或响应正文
- 分享密码不写入设置、日志、缓存或下载历史；后置配置失败会尝试撤销刚创建的链接
- MD5 不一致时返回稳定 `integrity_error` 且不发布最终文件；取消任务立即清理精确临时文件，暂停任务保持可恢复

## [2.0.0] - 2026-07-25

### Added
- 视频和音频文件的流式预览（边下边播）：点击播放按钮或双击媒体文件即可在应用内即时播放，无需等待完整下载
- 注册 `koofr-stream` 自定义 URI scheme 协议，Rust 侧代理 Koofr HTTP Range 请求，令牌不暴露给 WebView
- `MediaPreviewModal` 组件：全屏遮罩 + 原生 `<video>`/`<audio>` 控件，支持 seek、关闭自动清理流会话、不支持格式时提示下载
- `prepare_video_stream` / `close_video_stream` Tauri 命令和 `StreamSessionManager` 会话管理（30 分钟过期、断开连接时清空）

### Fixed
- 修复 React StrictMode 在播放器首次挂载时提前关闭流会话，导致实时播放返回 `403 session not found`

### Security
- CSP 新增 `media-src http://koofr-stream.localhost`，不放宽 `connect-src` 或其他指令
- 流会话 ID 是随机 UUID（非认证令牌），URI scheme handler 每次请求重新校验会话有效性

## [1.6.1] - 2026-07-24

### Added
- 应用工作目录选择，可在下次启动前选择全量迁移设置、传输状态、下载历史、缓存和日志

### Changed
- 缓存与日志固定收纳到应用工作目录；移除各自的路径选择，保留缓存策略和原有日志级别、保留期、轮转、占用与清理配置。
- 优化前端交互层次：结构栏采用克制的半透明材质，菜单、弹窗、传输入口和通知使用一致的空间动效，按钮在按下时立即反馈。
- 补齐减少动态、减少透明度和增强对比度偏好，并统一焦点环、选中态与文件列表悬停状态。

## [1.6.0] - 2026-07-23

### Added
- Koofr Vault 完整工作区：创建、导入/导出、原生 Safe Key 解锁、锁定、自动锁定、移除注册、解密浏览和文件操作
- 使用 [Koofr Vault](https://github.com/koofr/vault) 提供的 `vault-crypto` 实现 rclone crypt 兼容的 AES-EME 文件名与 XSalsa20-Poly1305 内容加密；加密上传可整文件恢复，密文下载支持 HTTP Range 字节续传
- Vault / rclone golden vector、Unicode + salt、块边界、篡改拒绝和 WebView 无 Safe Key 输入覆盖
### Security
- Vault Safe Key 只经过 Windows 原生凭据窗口和 Rust 内存；前端、日志、缓存、配置与传输检查点均不保存 Safe Key
- Vault 文件操作通过短期不透明句柄在 Rust 侧重新解析并限制到已注册根目录，密文路径和 validator 不暴露给 WebView

## [1.5.1] - 2026-07-23

### Added
- 文件图标大幅扩展覆盖面：新增视频（mp4、mkv、mov、avi、webm 等）、音频（mp3、flac、wav、ogg 等）、代码（ts/tsx、js、py、rs、go、java、c/cpp、sh、sql、yaml 等）、演示文稿（pptx/ppt/odp/key）、纯文本（txt、md、log、csv 等）、字体（ttf、otf、woff/woff2）、电子书（epub、mobi、azw3）、光盘映像（iso、img、vhd、vmdk）与数据库（sqlite、db、parquet）九类专属彩色图标
- 图片、压缩包、可执行文件、表格与文档的识别扩展名同步补全（heic、avif、zst、apk、deb、xlsm、docm 等）

### Changed
- 传输面板的文件类型识别改为复用 `fileKindByName`，与文件列表保持一致，不再维护两份扩展名表
- `.ts` 后缀按视频（MPEG-TS）识别，TypeScript 源码以 `.tsx` 识别为代码
- 传输面板只为具有恢复检查点的活动任务显示暂停按钮

## [1.5.0] - 2026-07-23

### Added
- 传输面板新增可选择的紧凑列表、文件来源与位置、开始/完成时间、平均速度和最近一分钟速度曲线
- 上传与下载速度曲线支持点击切换折线和平滑样式
- 下载历史按 Koofr 账户持久化，应用重启后仍可查看并打开已完成下载的位置

### Changed
- 下载入口角标只统计正在下载或等待网络重试的下载任务，不再统计历史记录、暂停任务或上传任务

## [1.4.0] - 2026-07-20

### Added
- 设置页新增“遇到网络错误时自动继续”开关，可设置最大重试次数（含无限）和固定重试间隔
- 设置页可配置缓存文件夹、日志文件夹、日志级别、保留天数和轮转大小，并可查看占用及清理日志
- 后台 JSONL 诊断日志，记录脱敏后的传输失败类别与关联 ID
- 顶层 `LICENSE`（MIT）、`CONTRIBUTING.md`、`SECURITY.md`
- `docs/ARCHITECTURE.md`：完整架构、数据流、传输恢复、安全边界
- `.github/ISSUE_TEMPLATE/`（bug / feature）与 `.github/PULL_REQUEST_TEMPLATE.md`
- 英文 README（`README.en.md`）

### Changed
- 网络自动恢复期间传输面板显示“等待网络重试”，且任务仍可暂停或取消
- 上传、下载和分卷上传仅在用户明确暂停时进入“已暂停”；网络、权限和本地 I/O 等真实错误现在显示为失败并保留重试入口
- 重写 `README.md`，按开源项目通用结构组织
- 精修 `docs/BUILDING.md` 与 `docs/RELEASING.md` 的措辞、目录与常见问题

## [1.3.3] - 2026-07-20

### Changes

- Fix transfer progress resetting to 0% when paused

## [1.3.2] - 2026-07-19

### Changes

- Fix Rust formatting required by the release workflow

## [1.3.1] - 2026-07-19

### Changes

- Add a pause button for active transfers
- Refactor the frontend structure

## [1.3.0] - 2026-07-19

### 变更

- 重构上传和下载文件后端
- 支持下载断点续传
- 支持上传时分卷拆分大文件并提供恢复方式

- Add sharing-link management for querying, creating, copying, and confirming revocation of download and file-receiving links
- Detect existing Koofr, Google Drive, OneDrive, Dropbox, and other connected storage mounts

## [1.1.0] - 2026-07-17

### 变更

- Add reusable inline SVG icons for folders and supported file types
- Detect archive and executable extensions in file listings
- Apply semantic icon tokens and document the new design variants
- Add coverage for all supported file kinds

## [1.0.0] - 2026-07-16

### 新增

- Koofr 应用专用密码登录，可选 Windows 凭据管理器存储，会话恢复，退出登录。
- 浏览挂载点、目录、最近文件、共享内容和回收站。
- 新建文件夹、上传、下载、重命名、移动、复制、删除、回收站恢复。
- 流式传输进度、取消、递归文件夹下载、未完成下载清理。
- 可配置元数据缓存，响应式 Windows 桌面界面。

### 安全

- 凭据始终留在 Rust 后端。可选的持久化存储用 Windows 凭据管理器。
- 发布构建在仓库质量检查通过后上传到 GitHub Release。当前不签名。

### 已知限制

- 1.0.0 不含 Koofr Vault 解锁、加密传输或 rclone-crypt 兼容性。
- 1.0.0 不含传输重试和续传。
- 安装包未签名，Windows 可能显示未知发布者或 SmartScreen 警告。
