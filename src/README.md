# 前端（React / TypeScript）

- `components/`：应用外壳与可复用 UI 组件。
- `features/auth/`：登录表单、客户端校验与安全提示。
- `features/files/`：Koofr 挂载点、目录状态、文件展示、操作与统一文件预览器。
- `features/links/`：下载链接与接收文件链接的查询、创建、撤销界面。
- `features/places/`：已连接存储的提供方识别与官方管理入口。
- `features/transfers/`：上传与下载传输队列界面。
- `features/sync/`：多组本地文件夹与 Koofr 主存储目录之间双向同步的管理、状态与问题列表。
- `features/vault/`：Vault 注册管理、解密目录浏览与加密文件操作界面。
- `services/`：类型化的 Tauri 命令与传输事件封装。
- `types/`：前端领域类型。

账户连接、启动会话检查与退出登录已通过 `services/koofr.ts` 接入 Rust 后端。应用专用密码在每次提交后即从表单状态中清除；仅当用户勾选“保存密码”时，Rust 后端才会将其写入 Windows 凭据管理器（服务名 `Koofr-GUI`，账号名为邮箱地址）。文件工作区读取真实挂载点与目录，已接入本地文件选择、上传、下载、新建、重命名与删除。最近文件、已共享、回收站列表以及恢复 / 清空操作也通过类型化后端命令读取真实账户数据。

`features/sync/SyncWorkspace.tsx` 通过 `services/koofr.ts` 管理经典同步与 Windows 按需同步组。创建或迁移时，本地目录只能来自 Rust 原生文件夹授权，远端仅显示在线主存储挂载点；同步状态由 `koofr://sync-state` 事件增量更新。详情窗口打开时通过 `list_sync_tasks` 按需读取当前文件显示名，文件名不会进入持续广播事件或诊断日志。移除经典配置不会删除文件；按需根会在解除注册前显示明确警告，说明 Windows 解除注册的语义。

`features/files/FilePreviewModal.tsx` 统一显示 PDF、纯文本 / 代码、图片、视频与音频。普通文件、最近文件与共享文件传入 `mountId + path`，Vault 传入既有不透明 entry handle；回收站不提供预览入口。PDF.js、DOMPurify 与 HEIC worker 均由 Vite 离线打包，文本仅以纯文本呈现。Office、OpenDocument、PSD、AI 与电子书不在预览解析范围内。

`features/vault/VaultDialog.tsx` 仅提交 Vault 注册 ID 与后端签发的短期 UUID 句柄，并显示后端解密后的名称、类型、明文大小与时间。Safe Key 输入由 Windows 原生凭据窗口完成，界面中不存在密码输入框，Safe Key 也不会进入 React 状态或 Tauri IPC。Vault 加密上传与可续传下载共用全局传输面板；上传的恢复语义为整文件重试，下载为密文字节 Range 续传。

文件工作区选中单个文件或文件夹后可直接创建分享链接，远程路径由所选项目提供，无需用户手动输入。侧栏“分享链接”仅负责查看、复制与撤销已有链接，通过独立的 `services/publicLinks.ts` 调用 Rust 命令。“已连接存储”读取现有挂载点，不在 WebView 中执行第三方 OAuth。需要新增、移除或重新授权时，仅显示 Koofr 官方账户地址引导用户前往操作。

`features/settings/` 提供 11 个已接入的分区。`App.tsx` 在启动时与登录恢复并行读取设置，并通过 props 协调即时语言、密度、动效、通知权限、托盘角标、传输悬浮窗、空闲登出、并发与限速、下载冲突、链接默认值与历史策略。悬浮窗设置可独立控制暂停或空闲时收为图标、单击批量暂停 / 继续以及双击打开应用。仅工作目录迁移仍明确在下次启动且任何工作文件打开之前由 Rust 完成。目录可手动填写，也可通过 Rust 打开的原生文件夹选择器获取；前端仅提交父目录，最终文件名、冲突避让与一次性路径授权均由 Rust 处理。

传输面板使用 Rust 侧的 `download-history.json` 恢复下载历史，并展示来源、最终本地位置、完整性校验、分卷进度、开始与完成时间、平均速度以及最近一分钟速度曲线。上传与下载均会采样速度，曲线可点击切换折线与平滑样式。独立的 `FloatingTransferWindow` 仅接收脱敏聚合快照，不含任务显示名或路径；整个表面可用左键拖动，右键请求 Rust 弹出原生应用菜单。并发进度按总字节加权，未知总量不会伪造百分比。历史记录按 Koofr 账户隔离；WebView 不使用 `localStorage` 保存本地路径。下载角标统计处于 `queued` / `running` / `retrying` 状态的下载任务，不包含已完成、暂停、失败或上传任务。
