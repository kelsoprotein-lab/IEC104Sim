#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const REPO = 'kelsoprotein-lab/IEC60870-5-104-Simulator'

const PLATFORM_PATTERNS = [
  { key: 'darwin-aarch64', re: /aarch64\.app\.tar\.gz$/ },
  { key: 'darwin-x86_64',  re: /x64\.app\.tar\.gz$/ },
  { key: 'windows-x86_64', re: /x64-setup\.nsis\.zip$/ },
  { key: 'linux-x86_64',   re: /amd64\.AppImage\.tar\.gz$/ },
]

export function groupAssetsByRole(assets) {
  const groups = { slave: {}, master: {} }
  const sigByUrl = new Map()
  for (const a of assets) {
    if (a.name.endsWith('.sig')) sigByUrl.set(a.name.slice(0, -4), a.browser_download_url)
  }
  for (const a of assets) {
    if (a.name.endsWith('.sig')) continue
    const role = a.name.startsWith('IEC104Slave_') ? 'slave'
              : a.name.startsWith('IEC104Master_') ? 'master' : null
    if (!role) continue
    const plat = PLATFORM_PATTERNS.find((p) => p.re.test(a.name))
    if (!plat) continue
    groups[role][plat.key] = {
      url: a.browser_download_url,
      sigUrl: sigByUrl.get(a.name),
    }
  }
  return groups
}

export function extractChangelogSection(md, version) {
  const lines = md.split('\n')
  const startRe = new RegExp(`^##\\s+${version.replace(/\./g, '\\.')}\\b`)
  let inSection = false
  const out = []
  for (const line of lines) {
    if (startRe.test(line)) { inSection = true; continue }
    if (inSection && /^##\s+/.test(line)) break
    if (inSection) out.push(line)
  }
  return out.join('\n').trim()
}

async function fetchSigContent(url) {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`fetch sig failed: ${url} ${res.status}`)
  return (await res.text()).trim()
}

async function main() {
  const tag = process.argv[2]
  if (!tag) { console.error('usage: gen-update-manifest.mjs <tag>'); process.exit(1) }
  const version = tag.replace(/^v/, '')

  const json = execFileSync('gh', ['api', `repos/${REPO}/releases/tags/${tag}`], { encoding: 'utf8' })
  const release = JSON.parse(json)
  const grouped = groupAssetsByRole(release.assets)

  const changelogPath = resolve(process.cwd(), 'CHANGELOG.md')
  const notes = extractChangelogSection(readFileSync(changelogPath, 'utf8'), version)
  const pubDate = release.published_at

  for (const role of ['slave', 'master']) {
    const platforms = {}
    for (const [key, val] of Object.entries(grouped[role])) {
      if (!val.sigUrl) {
        throw new Error(
          `missing .sig for ${role}/${key} (asset ${val.url}). ` +
          `Did the TAURI_SIGNING_PRIVATE_KEY secret get configured on the runner?`
        )
      }
      const sig = await fetchSigContent(val.sigUrl)
      platforms[key] = { signature: sig, url: val.url }
    }
    if (Object.keys(platforms).length === 0) {
      throw new Error(`no platforms found for role ${role}`)
    }
    const manifest = { version, notes, pub_date: pubDate, platforms }
    const out = resolve(process.cwd(), `latest-${role}.json`)
    writeFileSync(out, JSON.stringify(manifest, null, 2))
    console.log(`wrote ${out}`)
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => { console.error(e); process.exit(1) })
}
