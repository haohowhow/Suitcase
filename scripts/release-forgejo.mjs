import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { createReadStream, openAsBlob } from 'node:fs'
import { lstat, readFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const apiRoot = 'https://forgejo.zenwinhow.abrdns.com/api/v1/repos/sora-admin/Koofr-GUI'
const releaseUrl = 'https://forgejo.zenwinhow.abrdns.com/sora-admin/Koofr-GUI/releases'

export function releaseNotes(changelog, version) {
  const heading = `## [${version}]`
  const lines = changelog.replaceAll('\r\n', '\n').split('\n')
  const start = lines.findIndex((line) => line === heading || line.startsWith(`${heading} - `))
  if (start < 0) throw new Error('The version is missing from CHANGELOG.md.')
  const end = lines.findIndex((line, index) => index > start && line.startsWith('## ['))
  const notes = lines.slice(start + 1, end < 0 ? undefined : end).join('\n').trim()
  if (!notes) throw new Error('The release notes are empty.')
  return `${notes}\n\nUnsigned Windows x64 installer. Verify its SHA-256 against SHA256SUMS before running it.`
}

// Authenticated requests stay on this Forgejo origin and never follow redirects.
export function forgejoApi(token, fetchImpl = fetch) {
  return async (method, path, body) => {
    let response
    try {
      response = await fetchImpl(`${apiRoot}${path}`, {
        method,
        headers: {
          Authorization: `token ${token}`,
          Accept: 'application/json',
          ...(body && !(body instanceof FormData) ? { 'Content-Type': 'application/json' } : {}),
        },
        body: body instanceof FormData ? body : body ? JSON.stringify(body) : undefined,
        redirect: 'error',
        signal: AbortSignal.timeout(120_000),
      })
    } catch {
      throw new Error('Forgejo request failed; check connectivity and TLS. No credentials were logged.')
    }
    if (response.status === 404 && method === 'GET') return null
    if (!response.ok) throw new Error(`Forgejo request failed (HTTP ${response.status}).`)
    try { return await response.json() } catch {
      throw new Error('Forgejo returned an invalid JSON response.')
    }
  }
}

export async function uploadDraft({ tag, commit, body, assets }, api) {
  const remoteTag = await api('GET', `/tags/${encodeURIComponent(tag)}`)
  if (!remoteTag || remoteTag.commit?.sha !== commit) {
    throw new Error('Push the existing release tag to Forgejo first; its commit must match HEAD.')
  }
  let release = await api('GET', `/releases/tags/${encodeURIComponent(tag)}`)
  if (release && (!release.draft || release.assets?.length)) {
    throw new Error('The release already exists or has attachments; refusing to overwrite it.')
  }
  if (!release) {
    release = await api('POST', '/releases', {
      tag_name: tag,
      target_commitish: commit,
      name: `Koofr GUI ${tag}`,
      body,
      draft: true,
      prerelease: false,
    })
  }
  if (!Number.isSafeInteger(release.id) || release.id <= 0 || !release.draft) {
    throw new Error('Forgejo did not return a valid draft release.')
  }
  for (const asset of assets) {
    const form = new FormData()
    form.append('attachment', asset.blob, asset.name)
    await api('POST', `/releases/${release.id}/assets?name=${encodeURIComponent(asset.name)}`, form)
  }
  // Publishing remains a separate manual review in Forgejo.
  return release.id
}

function git(...args) {
  return execFileSync('git', args, {
    cwd: projectRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'ignore'],
    env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GCM_INTERACTIVE: 'Never' },
  }).trim()
}

async function main() {
  const args = process.argv.slice(2)
  if (args.some((arg) => arg !== '--dry-run')) {
    throw new Error('Usage: node scripts/release-forgejo.mjs [--dry-run]')
  }
  const dryRun = args.includes('--dry-run')
  const pkg = JSON.parse(await readFile(join(projectRoot, 'package.json'), 'utf8'))
  const tauri = JSON.parse(await readFile(join(projectRoot, 'src-tauri/tauri.conf.json'), 'utf8'))
  const cargo = await readFile(join(projectRoot, 'src-tauri/Cargo.toml'), 'utf8')
  const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(pkg.version)
      || pkg.version !== tauri.version || pkg.version !== cargoVersion) {
    throw new Error('All three version sources must match a stable semantic version.')
  }
  const tag = `v${pkg.version}`
  const body = releaseNotes(await readFile(join(projectRoot, 'CHANGELOG.md'), 'utf8'), pkg.version)
  const commit = git('rev-parse', 'HEAD')
  if (git('rev-parse', `${tag}^{commit}`) !== commit) {
    throw new Error('HEAD must match the existing version tag. This script never creates or pushes tags.')
  }
  if (!dryRun && git('status', '--porcelain')) {
    throw new Error('Commit or preserve local changes before uploading a release.')
  }
  const installerName = `${tauri.productName}_${pkg.version}_x64-setup.exe`
  const installerPath = join(projectRoot, 'src-tauri/target/release/bundle/nsis', installerName)
  let installerStat
  try { installerStat = await lstat(installerPath) } catch {
    throw new Error('The current installer is missing. Run npm run check and npm run build:installer first.')
  }
  if (!installerStat.isFile() || installerStat.isSymbolicLink() || !installerStat.size) {
    throw new Error('The installer must be a non-empty regular file, not a symbolic link.')
  }
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(installerPath)) hash.update(chunk)
  const checksum = `${hash.digest('hex')}  ${installerName}\n`
  console.log(`Tag: ${tag}\nDestination: ${releaseUrl}\nInstaller: ${installerName}\n${checksum.trim()}`)
  if (dryRun) {
    console.log('Dry run complete; no API requests or uploads were made.')
    return
  }
  const token = process.env.FORGEJO_TOKEN
  if (!token) throw new Error('Set FORGEJO_TOKEN locally; never pass it as a command-line argument.')
  const id = await uploadDraft({
    tag, commit, body,
    assets: [
      { name: installerName, blob: await openAsBlob(installerPath) },
      { name: 'SHA256SUMS', blob: new Blob([checksum], { type: 'text/plain' }) },
    ],
  }, forgejoApi(token))
  console.log(`Draft release ${id} uploaded. Review both attachments in ${releaseUrl} and publish manually.`)
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : 'Release upload failed.')
    process.exitCode = 1
  })
}
