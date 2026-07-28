import type { InterfaceLanguage } from './types/backend'

const enUs: Record<string, string> = {
  '我的文件': 'My files',
  '最近': 'Recent',
  '与我共享': 'Shared with me',
  '分享链接': 'Shared links',
  '回收站': 'Trash',
  '设置': 'Settings',
  '退出登录': 'Sign out',
  '上传': 'Upload',
  '下载': 'Download',
  '新建文件夹': 'New folder',
  '刷新': 'Refresh',
  '搜索': 'Search',
  '名称': 'Name',
  '大小': 'Size',
  '修改时间': 'Modified',
  '外观': 'Appearance',
  '常规': 'General',
  '传输': 'Transfers',
  '带宽': 'Bandwidth',
  '通知': 'Notifications',
  '安全': 'Security',
  '历史': 'History',
  '存储与缓存': 'Storage and cache',
  '日志': 'Logs',
  '关于': 'About',
  '界面语言': 'Interface language',
  '切换后立即刷新应用界面': 'The app interface refreshes immediately',
  '跟随系统': 'Use system language',
  '简体中文': 'Simplified Chinese',
  '英文': 'English',
  '减少动态效果': 'Reduce motion',
  '界面密度': 'Interface density',
  '标准': 'Standard',
  '紧凑': 'Compact',
  '开机时启动': 'Launch at login',
  '关闭窗口时最小化到托盘': 'Close window to tray',
  '最小化时隐藏到托盘': 'Hide in tray when minimized',
  '有传输任务时退出前确认': 'Confirm before quitting with active transfers',
  '上传并发数': 'Concurrent uploads',
  '下载并发数': 'Concurrent downloads',
  '默认分卷大小': 'Default part size',
  '自动分卷提示阈值': 'Split-upload prompt threshold',
  '提示改用分卷的阈值': 'Prompt to use split upload above',
  '冲突处理': 'Conflict handling',
  '每次询问': 'Ask every time',
  '跳过': 'Skip',
  '自动加序号': 'Add a number automatically',
  '下载完成后打开文件夹': 'Open folder after download',
  '覆盖单文件、文件夹、Vault 与恢复下载': 'Applies to files, folders, Vault, and resumed downloads',
  '下载后校验文件完整性': 'Verify file integrity after download',
  '失败分片保留天数': 'Days to keep failed partial files',
  '不限速': 'Unlimited',
  '限速': 'Limited',
  '系统空闲时全速': 'Full speed while system is idle',
  '全局速度上限': 'Global speed limit',
  '单任务速度上限': 'Per-task speed limit',
  '传输完成时通知': 'Notify when transfers finish',
  '使用 Tauri 原生通知，仅包含任务显示名，不包含完整文件路径。': 'Uses native Tauri notifications with the task display name, never a full file path.',
  '播放通知声音': 'Play notification sound',
  '显示任务栏角标': 'Show taskbar badge',
  '显示传输悬浮窗': 'Show transfer floating window',
  '置顶显示上传、下载或暂停状态；单击可打开传输面板': 'Keep upload, download, or paused status on top; click to open Transfers',
  '显示传输悬浮窗？': 'Show the transfer floating window?',
  '开启悬浮窗': 'Enable floating window',
  '正在开启…': 'Enabling…',
  '暂不开启': 'Not now',
  '悬浮窗会置顶显示上传、下载和暂停状态，只同步任务数量与汇总百分比，不包含文件名或路径。': 'The floating window stays on top with upload, download, and paused status. It only receives task counts and aggregate progress, never file names or paths.',
  '可拖动到合适位置，单击即可回到传输面板。': 'Drag it to a convenient position and click it to return to Transfers.',
  '链接默认类型': 'Default link type',
  '下载链接': 'Download link',
  '接收文件链接': 'File request link',
  '默认有效期': 'Default expiration',
  '要求密码': 'Require password',
  '空闲自动退出登录': 'Sign out after inactivity',
  '保留天数': 'Retention days',
  '保留条数': 'Maximum entries',
  '清理已完成记录': 'Clear finished entries',
  '缓存模式': 'Cache mode',
  '关闭缓存': 'Cache off',
  '仅内存': 'Memory only',
  '磁盘缓存': 'Disk cache',
  '清空缓存': 'Clear cache',
  '清空日志': 'Clear logs',
  '项目主页': 'Project homepage',
  '发布页': 'Releases',
  '反馈问题': 'Report an issue',
  '保存': 'Save',
  '取消': 'Cancel',
  '完成': 'Done',
  '排队中': 'Queued',
  '正在传输': 'Transferring',
  '等待网络重试': 'Waiting to retry',
  '分卷进度': 'Part progress',
  '已暂停': 'Paused',
  '已完成': 'Completed',
  '已取消': 'Cancelled',
  '失败': 'Failed',
  '暂停': 'Pause',
  '继续': 'Resume',
  '打开文件夹': 'Open folder',
  '打开文件': 'Open file',
  '登录': 'Sign in',
  '邮箱': 'Email',
  '应用专用密码': 'App password',
  '记住我': 'Remember me',
  '连接 Koofr': 'Connect to Koofr',
  '创建链接': 'Create link',
  '复制链接': 'Copy link',
  '链接已创建': 'Link created',
  '一次性生成的分享密码': 'One-time generated share password',
  '播放': 'Play',
  '预览': 'Preview',
  '关闭': 'Close',
  '下载后打开': 'Download and open',
  '上一页': 'Previous page',
  '下一页': 'Next page',
  '页码': 'Page number',
  '适应宽度': 'Fit width',
  '编码': 'Encoding',
  '文本编码': 'Text encoding',
  '自动检测': 'Auto detect',
  '自动换行': 'Wrap lines',
  '缩小': 'Zoom out',
  '放大': 'Zoom in',
  '适应窗口': 'Fit window',
  '实际大小': 'Actual size',
  '旋转': 'Rotate',
  '正在加载预览…': 'Loading preview…',
  '文件较大，仅显示前 5 MiB。': 'This file is large; only the first 5 MiB is shown.',
  '检测到无法按当前编码解码的字符。': 'Some characters could not be decoded with the selected encoding.',
  '无法解析此 PDF，请下载后打开。': 'This PDF could not be parsed. Download it to open.',
  '无法读取此文本文件，请下载后打开。': 'This text file could not be read. Download it to open.',
  '无法解析此图片，请下载后打开。': 'This image could not be decoded. Download it to open.',
  '无法播放此格式，请下载后使用系统播放器打开。': 'This media format cannot be played here. Download it to use a system player.',
  '重命名': 'Rename',
  '移动': 'Move',
  '复制': 'Copy',
  '删除': 'Delete',
}

const zhCn = Object.fromEntries(Object.entries(enUs).map(([zh, en]) => [en, zh]))
const translatedAttributes = ['aria-label', 'placeholder', 'title'] as const

export function resolvedLanguage(language: InterfaceLanguage): 'zh-CN' | 'en-US' {
  if (language === 'zhCn') return 'zh-CN'
  if (language === 'enUs') return 'en-US'
  return navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US'
}

export function installRuntimeTranslations(
  root: HTMLElement,
  language: InterfaceLanguage,
) {
  const resolved = resolvedLanguage(language)
  document.documentElement.lang = resolved
  const resource = resolved === 'en-US' ? enUs : zhCn
  const translateText = (value: string) => {
    const leading = value.match(/^\s*/)?.[0] ?? ''
    const trailing = value.match(/\s*$/)?.[0] ?? ''
    const content = value.trim()
    return resource[content] ? `${leading}${resource[content]}${trailing}` : value
  }
  const translateNode = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE && node.textContent) {
      const translated = translateText(node.textContent)
      if (translated !== node.textContent) node.textContent = translated
      return
    }
    if (!(node instanceof HTMLElement)) return
    for (const attribute of translatedAttributes) {
      const value = node.getAttribute(attribute)
      if (value && resource[value]) node.setAttribute(attribute, resource[value])
    }
    for (const child of node.childNodes) translateNode(child)
  }
  translateNode(root)
  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) translateNode(node)
      if (mutation.type === 'characterData') translateNode(mutation.target)
    }
  })
  observer.observe(root, { childList: true, subtree: true, characterData: true })
  return () => observer.disconnect()
}
