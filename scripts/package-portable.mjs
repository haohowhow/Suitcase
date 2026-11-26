import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { createReadStream } from 'node:fs'
import { cp, lstat, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')

async function main() {
  const pkg = JSON.parse(await readFile(join(projectRoot, 'package.json'), 'utf8'))
  const tauri = JSON.parse(await readFile(join(projectRoot, 'src-tauri/tauri.conf.json'), 'utf8'))
  const cargo = await readFile(join(projectRoot, 'src-tauri/Cargo.toml'), 'utf8')
  const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  if (
    !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(pkg.version) ||
    pkg.version !== tauri.version ||
    pkg.version !== cargoVersion
  ) {
    throw new Error('All three version sources must match a stable semantic version.')
  }

  const binaryName = `${pkg.name}.exe`
  const binaryPath = join(projectRoot, 'src-tauri/target/release', binaryName)
  let binaryStat
  try {
    binaryStat = await lstat(binaryPath)
  } catch {
    throw new Error(`Release binary not found at ${binaryPath}. Run npm run build:desktop first.`)
  }
  if (!binaryStat.isFile() || binaryStat.isSymbolicLink() || !binaryStat.size) {
    throw new Error('The release binary must be a non-empty regular file, not a symbolic link.')
  }

  const bundleDir = join(projectRoot, 'src-tauri/target/release/bundle/portable')
  const folderName = `${tauri.productName}_${pkg.version}_x64-portable`
  const stageDir = join(bundleDir, folderName)
  const archiveName = `${folderName}.zip`
  const archivePath = join(bundleDir, archiveName)

  await rm(stageDir, { recursive: true, force: true })
  await rm(archivePath, { force: true })
  await mkdir(stageDir, { recursive: true })

  await cp(binaryPath, join(stageDir, binaryName))
  await cp(join(projectRoot, 'LICENSE'), join(stageDir, 'LICENSE'))
  await cp(join(projectRoot, 'THIRD_PARTY_NOTICES.md'), join(stageDir, 'THIRD_PARTY_NOTICES.md'))
  await cp(join(projectRoot, 'licenses'), join(stageDir, 'licenses'), { recursive: true })
  await cp(
    join(projectRoot, 'src-tauri/third_party/libraw-0.21.1'),
    join(stageDir, 'third_party/libraw-0.21.1'),
    { recursive: true },
  )

  execFileSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-NonInteractive',
      '-Command',
      'Compress-Archive -LiteralPath $env:SUITCASE_STAGE_DIR -DestinationPath $env:SUITCASE_ARCHIVE_PATH -CompressionLevel Optimal -Force',
    ],
    {
      cwd: projectRoot,
      stdio: 'inherit',
      env: {
        ...process.env,
        SUITCASE_STAGE_DIR: stageDir,
        SUITCASE_ARCHIVE_PATH: archivePath,
      },
    },
  )

  await rm(stageDir, { recursive: true, force: true })

  const hash = createHash('sha256')
  for await (const chunk of createReadStream(archivePath)) hash.update(chunk)
  const checksumLine = `${hash.digest('hex')}  ${archiveName}\n`
  const checksumPath = join(bundleDir, 'SHA256SUMS')
  await writeFile(checksumPath, checksumLine, 'utf8')

  console.log(`Portable archive: ${archivePath}`)
  console.log(`SHA256SUMS: ${checksumLine.trim()}`)
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : 'Portable packaging failed.')
    process.exitCode = 1
  })
}
