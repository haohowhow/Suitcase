# 更新日志

格式按 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，版本号按 [SemVer](https://semver.org/spec/v2.0.0.html)。

## [Unreleased]

### Changed
- 新增项目进度快照与可随仓库分发的 Agent 接手指南，整理版本演进、同步功能边界、代码入口、验证与人工验收要求；补齐中英文 README 与贡献指南的交接入口，并校正架构文档中的传输状态及已有前端窗口调用例外。
- 项目托管迁移至自建 Forgejo，更新应用内主页、下载和问题反馈入口、文档链接与贡献模板；安全报告改用维护者邮箱。
- 移除依赖 GitHub 托管 Windows runner 的工作流，改为 Windows 本地检查、构建和上传 Forgejo Release 草稿，安装包同时附带 SHA-256 校验文件。

### Fixed
- 修复首次同步后新建的文件夹被误判为删除：本地新建（含空文件夹）现在会在 Koofr 创建，网页或其他设备新建的远端文件夹会在本地创建，不再被条件删除或让同步组反复卡在扫描中。
- 修复本地删除文件夹总被当作冲突，导致远端删除文件夹后同步组永久停在“扫描中”且基线不再更新的问题。目录在子项处理完后若为空则删除；若仍含未同步或被忽略的本地内容，则恢复远端文件夹，不丢弃本地数据。
- 修复同步进行中发生的修改或删除被写入基线后不再传播、两端永久静默不一致的问题。基线现在只记录两端一致的条目，分歧条目保留上一轮基线，下一轮扫描会继续上传、下载、删除或生成冲突副本；经典同步与按需同步均适用。
- 同步下载在替换本地文件前再次核对本地状态：下载期间新建、修改或删除的本地文件不再被覆盖并永久删除，而是转为冲突重扫。
- 按需同步删除占位符目录不再递归永久删除，只移除空目录；目录内仍有被忽略或未上传的本地文件时恢复远端文件夹。删除子项导致目录失去同步标记时不再误判冲突。
- 扫描或最终复扫遇到非网络错误（如远端根目录不存在）时，同步组进入“需要处理”并记录问题，不再一直显示“扫描中”；网络与登录失效统一显示为离线。
- 条件操作发现冲突后立即重新扫描（连续最多 3 次），不再等待下一个 60 秒周期。
- 部分操作失败时仍保存两端已一致的基线，单个持续失败的文件不再让基线永久停止更新、删除永久无法传播；按需同步遇到单项失败后继续处理其余项目。相同路径与错误码的未解决问题不再每轮重复记录。
- 按需水合续传时核对响应的 `ETag` / `Last-Modified`，远端版本变化即中止，避免把不同版本的数据写进同一个占位符；水合结束后释放限速器中的任务计数。
- 前端同步完成计数不再在 StrictMode 或短时间连续事件下重复累加。
- 修复文件预览在同步或传输进行时反复重载：同步速度采样的定时器每秒都让整个界面重新渲染，而文本和图片预览又会在每次渲染后重新拉取内容，导致文本预览不停闪烁、稍大的图片始终加载不出来。定时器在没有采样过期时不再触发渲染，预览也不再因父组件重新渲染而重新下载。
- 修复上传或恢复上传完成后界面被拉回发起时所在文件夹的问题：现在只有仍停留在目标文件夹时才刷新列表。
- 修复界面翻译误改用户数据：名为 `Download`、`Logs`、`设置` 等恰好与界面词条相同的文件、文件夹、存储位置和保险箱，以及内容恰好相同的文本预览，不再被显示成译文。
- 传输悬浮窗的批量暂停不再作用于不支持续传的文件夹下载，避免留下既不能继续也无法清除的“已暂停”记录。
- 空闲自动退出登录在有传输进行时顺延，不再中途打断正在运行的传输。
- 预览失败的提示现在会自动消失；传输完成通知和“下载后打开文件夹”不再有重复触发的可能。
- 修复同名下载互相覆盖：选择文件名时会避开仍在进行或已暂停的下载、以及已分配但尚未开始的下载目标；发布到最终位置时也不再替换已有文件，名称在下载期间被占用时自动改用带序号的名称，下载历史与“打开文件/文件夹”指向实际位置。文件夹下载同样如此。
- 设置、元数据缓存、传输检查点、下载历史与工作目录记录改为原子写入：每次写入使用独立临时文件并一次性替换，多个传输同时更新也不会互相覆盖，写入途中崩溃不会丢失原文件。文件无法解析时改名为 `*.corrupt-<时间戳>` 保留，并在诊断日志记录，不再被下次保存静默清空。
- 已取消的上传不会在重启后重新出现；服务器拒绝（无权限、冲突、目标不存在）或输入无效的上传也不再保留检查点。检查点存满 128 条时淘汰最旧的已停止整文件上传记录，不再让所有新传输报“无法保存本地应用数据”；单个分片文件不可读时不再导致整个可续传列表加载失败。
- 修复传输在开始后的少数失败路径（读取分片、写检查点等）中提前返回却未释放传输 ID，导致同一任务无法再次开始、暂停或继续；排队中被暂停的下载现在正确显示为“已暂停”，排队中取消的下载会清理分片与检查点。
- 网络连接卡住时不再永久挂起：元数据请求最长等待 60 秒；下载、预览和同步下载在 60 秒内收不到响应头或任何数据时按网络错误处理并参与自动重试；上传 5 分钟没有任何进度时同样中止并可重试。连接启用 TCP keepalive。
- 下载历史校正时不再把正在运行的传输误改为已暂停或失败；没有检查点的“已暂停”记录（如文件夹下载）会转为失败，可以清除。
- 保险箱下载先发布解密后的文件再删除密文分片，发布失败时仍可续传而不必重新下载。
- 预览 0 字节文件不再导致预览代理越界；文件夹下载清理暂存目录失败时报告原始错误而不是清理错误。
- 为账户隔离键的回退哈希添加固定值测试，防止标准库哈希算法变化后同步组、检查点与下载历史与账户失联。
- 退出应用、退出登录（含空闲自动退出）和切换账户时，进行中的传输改为暂停而不是取消：可续传的下载保留分片和续传点，再次登录后可继续；退出前最多等待 3 秒让传输保存状态，并相应更新退出确认提示。
- 关闭“关闭到托盘”后，点主窗口关闭按钮会退出整个应用，不再只关掉窗口而让进程留在后台。
- 开机自启动且开启“关闭到托盘”时，启动后只驻留托盘，不再每次开机弹出主窗口。
- 安装包卸载钩子文件改为带 BOM 的 UTF-8，避免 NSIS 按系统代码页读取导致中文卸载提示乱码（需在下次打包后人工确认）。

## [2.6.0] - 2026-08-21

### Added
- 新增 Windows 原生按需同步预览：使用 Cloud Files API 注册同步根、按目录创建在线占位符、打开时水合文件，并接入资源管理器内置的在线 / 已下载 / 始终保留 / 错误状态与系统菜单。
- 同时支持映射主存储 `/` 的统一 Koofr 根和多个互不重叠的自定义映射；新增经典同步到新空目录按需根的可回滚迁移、资源管理器打开入口，以及 NSIS 卸载前同步根清理。

### Changed
- 同步 SQLite 增加模式、根类型、迁移状态和不透明占位符条目表；占位符身份只包含版本与随机 UUID，远端路径和校验信息继续保存在账户隔离的本地数据库。
- 本地扫描识别本应用的 CFAPI 占位符，干净占位符不再被打开或哈希；按需水合复用下载并发、限速、取消、MD5 校验和 4 KiB 对齐写入。
- 存在按需同步根时，开机启动和关闭到托盘显示为原生同步所需；完全退出仍会断开 Provider，但保留同步根注册。
- 按需同步默认进入可浏览的“选择 Koofr 文件夹”流程；“同步整个 Koofr”改为明确的独立范围，不再显示不可操作的远端输入框。

### Fixed
- 经典同步与按需同步详情现在显示当前文件、操作类型、处理序号、剩余数量和分方向队列；总进度优先按任务数计算，不再因文件字节已传完而提前显示 100%。任务显示名仅在详情打开时按需读取，不进入同步事件或日志。
- 修复 CFAPI 目录填充回调遗漏 DOS 卷名，导致在资源管理器打开按需同步根时提示云端加载错误的问题。
- 按需同步现在处理 CFAPI 删除前通知：资源管理器删除占位符时先校验同步组、路径和不透明条目身份，再立即确认 Windows 删除；Koofr 条件删除在异步任务中继续执行，并由常规扫描负责失败后的重试与协调，避免回收站进度框因等待网络而长期卡在 0%，也不会把未变更的远端文件自动补回。

### Security
- 注册更新、解除注册和卸载清理都会核对固定 Provider 名称与版本化根身份，不会修改其他云盘的同步根；日志和同步事件仍不包含凭据、邮箱、文件名或完整路径。

## [2.5.0] - 2026-08-06

### Added
- 新增 Windows 本地文件夹同步：支持多组本地目录与 Koofr 主存储普通目录双向同步、`.syncignore`、首次合并、冲突副本、条件删除 / 回收站、暂停 / 立即同步、问题列表以及 `sync-state.sqlite3` 持久化状态。

### Changed
- 同步详情增加本地与云端文件统计、当前周期进度、上下行实时 / 平均 / 峰值速度和最近 60 秒曲线；同步状态汇总接入统一悬浮活动窗。
- 远端目录选择器改为带返回和选择按钮的键盘可用弹层；同步卡片显示双端统计、进度与详情入口。
- 同步扫描显示上次有效基线并在扫描过程中渐进更新；未变化本地文件复用已缓存的 MD5，远端目录采用有限并发扫描。

### Fixed
- 修复同步协调循环可能导致 Tokio worker 栈溢出的问题，并避免启动时立即重复执行一次全量扫描。
- 修复扫描未完成时详情长期显示双端 0 统计的问题；取消同步操作不再被记录为文件问题。

## [2.4.0] - 2026-08-02

### Changed
- 对项目整体文档（包括 README、构建与发布指南、安全策略、贡献指南及第三方声明等）进行全面整理与专业化修正，确保技术参数、安全边界与代码实现保持一致。
- Rust 传输引擎支持 IPC 进度事件节流控制（80ms 最小间隔），在高速和多分块传输场景下大幅降低 IPC 消息推送频率与 CPU 开销。
- 重构前端状态架构：提取 `useAppSettings` 和 `useTransferManager` 自定义 Hooks，解耦 `App.tsx` 逻辑。
- 优化前端渲染性能：针对 `FileWorkspace`、`CollectionWorkspace`、`TransferPanel`、`SettingsPanel`、`AppSidebar`、`TitleBar`、`LoginPage` 增加 `React.memo` 记忆化保护，配合 `useCallback` 切断传输事件对无关 UI 的重渲染。

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
