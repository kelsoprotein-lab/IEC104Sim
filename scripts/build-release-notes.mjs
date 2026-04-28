#!/usr/bin/env node
// Generate a richly formatted GitHub Release body for a given tag.
// Usage: node scripts/build-release-notes.mjs <tag>     # writes RELEASE_BODY.md
//
// Produces:
//   - Header (version + date)
//   - Per-OS download table (slave / master)
//   - Verbatim CHANGELOG section for the version
//   - Footer with links to full changelog + previous releases
//
// Designed to be called from CI after build jobs upload assets, so the
// rendered table always lines up with what's actually in the release.

import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { extractChangelogSection } from './gen-update-manifest.mjs'

const REPO = 'kelsoprotein-lab/IEC60870-5-104-Simulator'
const REPO_URL = `https://github.com/${REPO}`

const APPS = [
  { role: 'Slave (从站)', prefix: 'IEC104Slave' },
  { role: 'Master (主站)', prefix: 'IEC104Master' },
]

const PLATFORMS = [
  { label: 'macOS Apple Silicon', file: (p, v) => `${p}_${v}_aarch64.dmg` },
  { label: 'macOS Intel',         file: (p, v) => `${p}_${v}_x64.dmg` },
  { label: 'Windows x64 (NSIS)',  file: (p, v) => `${p}_${v}_x64-setup.exe` },
  { label: 'Windows x64 (MSI)',   file: (p, v) => `${p}_${v}_x64_en-US.msi` },
  { label: 'Linux AppImage',      file: (p, v) => `${p}_${v}_amd64.AppImage` },
  { label: 'Linux deb',           file: (p, v) => `${p}_${v}_amd64.deb` },
  { label: 'Linux rpm',           file: (p, v) => `${p === 'IEC104Slave' ? 'IEC104Slave' : 'IEC104Master'}-${v}-1.x86_64.rpm` },
]

export function buildBody(tag, changelog) {
  const version = tag.replace(/^v/, '')
  const section = extractChangelogSection(changelog, version)

  const lines = []
  lines.push(`# IEC60870-5-104 Simulator ${tag}`)
  lines.push('')
  lines.push('## 下载 / Downloads')
  lines.push('')
  lines.push('下方资产里按平台选择 / Pick the asset for your platform below:')
  lines.push('')
  lines.push(`| 平台 / Platform | ${APPS[0].role} | ${APPS[1].role} |`)
  lines.push('|---|---|---|')
  for (const p of PLATFORMS) {
    const cells = APPS.map((a) => `\`${p.file(a.prefix, version)}\``).join(' | ')
    lines.push(`| ${p.label} | ${cells} |`)
  }
  lines.push('')
  if (section) {
    lines.push(section)
    lines.push('')
  } else {
    lines.push(`> ⚠️  CHANGELOG.md 缺少 \`${version}\` 的 section,请补上。`)
    lines.push('')
  }
  lines.push('---')
  lines.push('')
  lines.push(`完整变更历史 / Full changelog: [CHANGELOG.md](${REPO_URL}/blob/main/CHANGELOG.md)`)
  lines.push('')
  lines.push(`之前版本 / Previous releases: <${REPO_URL}/releases>`)
  return lines.join('\n')
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const tag = process.argv[2]
  if (!tag) { console.error('usage: build-release-notes.mjs <tag>'); process.exit(1) }
  const changelog = readFileSync(resolve(process.cwd(), 'CHANGELOG.md'), 'utf8')
  const body = buildBody(tag, changelog)
  const out = resolve(process.cwd(), 'RELEASE_BODY.md')
  writeFileSync(out, body + '\n')
  console.log(`wrote ${out} (${body.length} bytes)`)
}
