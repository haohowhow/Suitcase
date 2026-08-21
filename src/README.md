# 前端（React / TypeScript）

- `components/`：应用外壳和可复用 UI 组件。
- `features/auth/`：登录表单、客户端校验、安全提示。
- `features/files/`：Koofr 挂载点、目录状态、文件展示、操作和统一文件预览器。
- `features/links/`：下载链接和接收文件链接的查询、创建、撤销界面。
- `features/places/`：已连接存储的提供方识别和官方管理入口。
- `features/transfers/`：上传/下载传输队列界面。
- `features/sync/`：多组本地文件夹 ↔ Koofr 主存储目录的双向同步管理、状态和问题列表。
- `features/vault/`：Vault 注册管理、解密目录浏览和加密文件操作界面。
- `services/`：类型化的 Tauri 命令和传输事件封装。
- `types/`：前端领域类型。

账户连接、启动会话检查和退出登录已通过 `services/koofr.ts` 接到 Rust 后端。应用专用密码每次提交完成后从表单状态清掉；只有用户勾了"保存密码"，Rust 后端才会把它写入 Windows 凭据管理器（服务名为 `Koofr-GUI`，账号名为邮箱地址）。文件工作区读取真实挂载点和目录，已经接入了本地文件选择、上传、下载、新建、重命名和删除。最近文件、已共享、回收站列表和恢复/清空操作也通过类型化后端命令读取真实账户数据。

`features/sync/SyncWorkspace.tsx` 通过 `services/koofr.ts` 管理经典与 Windows 按需同步组。创建 / 迁移时本地目录只能来自 Rust 原生文件夹授权，远端仅显示在线主存储挂载点；同步状态由 `koofr://sync-state` 事件增量更新。详情窗口打开时通过 `list_sync_tasks` 按需读取当前文件显示名，文件名不进入持续广播事件或诊断日志。经典配置移除不删除文件；按需根会先显示 Windows 解除注册语义的明确警告。

`features/files/FilePreviewModal.tsx` 统一显示 PDF、纯文本 / 代码、图片、视频和音频。普通文件、最近文件和共享文件传入 `mountId + path`，Vault 传入现有不透明 entry handle；回收站没有预览入口。PDF.js、DOMPurify 和 HEIC worker 都由 Vite 离线打包，文本只以纯文本呈现。Office、OpenDocument、PSD、AI 与电子书不尝试解析。

`features/vault/VaultDialog.tsx` 只提交 Vault 注册 ID 和后端签发的短期 UUID 句柄，并显示后端解密后的名称、类型、明文大小和时间。Safe Key 输入由 Windows 原生凭据窗口完成，不存在密码输入框，也不会进入 React 状态或 Tauri IPC。Vault 加密上传和可续传下载共用全局传输面板；上传的恢复语义是整文件重试，下载是密文字节 Range 续传。

文件工作区选中单个文件或文件夹后可以直接创建分享链接，远程路径由所选项目提供，不需要用户手敲。侧栏"分享链接"只负责查看、复制和撤销已有链接，通过独立的 `services/publicLinks.ts` 调 Rust 命令。"已连接存储"读取现有挂载点，不在 WebView 里做第三方 OAuth。需要新增、移除或重新授权时，只显示 Koofr 官方账户地址引导用户去操作。

`features/settings/` 提供 11 个已接线分区。`App.tsx` 在启动时与登录恢复并行读取设置，并通过 props 协调即时语言、密度、动效、通知权限、托盘角标、传输悬浮窗、空闲登出、并发 / 限速、下载冲突、链接默认值和历史策略。悬浮窗设置可独立控制暂停或空闲时收为图标、单击批量暂停/继续以及双击打开应用。只有工作目录迁移继续明确在下次启动且任何工作文件打开前由 Rust 完成。目录可以手填，也可以通过 Rust 打开的原生文件夹选择器获取；前端只提交父目录，最终文件名、冲突避让和一次性路径授权都由 Rust 处理。

传输面板使用 Rust 侧的 `download-history.json` 恢复下载历史，并展示来源、最终本地位置、完整性校验、分卷进度、开始/完成时间、平均速度和最近一分钟速度曲线。上传与下载都会采样速度，曲线可点击切换折线和平滑样式。独立的 `FloatingTransferWindow` 只接收脱敏聚合快照，不含任务显示名或路径；整个表面左键拖动，右键请求 Rust 弹出原生应用菜单。并发进度按总字节加权，未知总量不会伪造百分比。历史记录按 Koofr 账户隔离；WebView 不用 `localStorage` 保存本地路径。下载角标统计 `queued` / `running` / `retrying` 的下载任务，不包含已完成、暂停、失败或上传任务。
