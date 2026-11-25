export type PreviewCandidate = {
  readonly name: string
  readonly contentType: string
  readonly entryType: string
}

const PREVIEW_EXTENSIONS = new Set([
  'pdf',
  'txt', 'md', 'markdown', 'rst', 'rtf', 'log', 'csv', 'tsv', 'tex', 'bib', 'org', 'adoc',
  'asciidoc', 'js', 'jsx', 'mjs', 'cjs', 'ts', 'tsx', 'json', 'jsonc', 'json5', 'html', 'htm',
  'xhtml', 'xml', 'xsl', 'xslt', 'css', 'scss', 'sass', 'less', 'py', 'pyw', 'rb', 'php', 'go',
  'rs', 'toml', 'java', 'kt', 'kts', 'scala', 'groovy', 'gradle', 'c', 'h', 'cc', 'cpp', 'cxx',
  'hh', 'hpp', 'hxx', 'm', 'mm', 'cs', 'fs', 'fsx', 'vb', 'swift', 'dart', 'sh', 'bash', 'zsh',
  'fish', 'sql', 'lua', 'pl', 'pm', 'r', 'jl', 'ex', 'exs', 'erl', 'elm', 'clj', 'cljs', 'hs',
  'yaml', 'yml', 'ini', 'cfg', 'conf', 'env', 'properties', 'vue', 'svelte', 'astro',
  'png', 'jpg', 'jpeg', 'jfif', 'gif', 'webp', 'bmp', 'ico', 'avif', 'svg', 'tif', 'tiff',
  'heic', 'heif', 'raw', 'cr2', 'cr3', 'nef', 'nrw', 'arw', 'srf', 'sr2', 'raf', 'orf', 'rw2',
  'dng', 'pef', '3fr', 'erf', 'mef', 'mos', 'mrw', 'x3f',
  'mp4', 'm4v', 'mov', 'avi', 'wmv', 'flv', 'mkv', 'webm', 'mpeg', 'mpg', 'mpe', 'mts', 'm2ts',
  '3gp', '3g2', 'vob', 'ogv', 'rm', 'rmvb', 'asf', 'f4v',
  'mp3', 'wav', 'flac', 'aac', 'm4a', 'm4b', 'ogg', 'oga', 'opus', 'wma', 'aiff', 'aif', 'ape',
  'alac', 'amr', 'mid', 'midi', 'dsd', 'dsf', 'dff',
])

const DOWNLOAD_ONLY_EXTENSIONS = new Set([
  'doc', 'docx', 'docm', 'dot', 'dotx', 'dotm',
  'xls', 'xlsx', 'xlsm', 'xlsb', 'xlt', 'xltx', 'xltm',
  'ppt', 'pptx', 'pptm', 'pot', 'potx', 'potm', 'pps', 'ppsx',
  'odt', 'ods', 'odp', 'pages', 'numbers', 'key', 'wps',
  'psd', 'psb', 'ai', 'eps',
  'epub', 'mobi', 'azw', 'azw3', 'kfx', 'fb2', 'lit', 'lrf', 'ibooks',
])

export function isPreviewableFile(file: PreviewCandidate) {
  if (file.entryType !== 'file') return false
  const dot = file.name.lastIndexOf('.')
  const extension = dot > 0 ? file.name.slice(dot + 1).toLowerCase() : ''
  if (DOWNLOAD_ONLY_EXTENSIONS.has(extension)) return false
  const contentType = file.contentType.trim().toLowerCase()
  if (
    contentType.startsWith('text/')
    || contentType.startsWith('image/')
    || contentType.startsWith('video/')
    || contentType.startsWith('audio/')
    || ['application/pdf', 'application/json', 'application/xml', 'application/yaml'].includes(contentType)
  ) {
    return true
  }
  return PREVIEW_EXTENSIONS.has(extension)
}
