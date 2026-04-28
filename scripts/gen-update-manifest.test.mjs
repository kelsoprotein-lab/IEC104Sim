import { describe, it, expect } from 'vitest'
import { groupAssetsByRole, extractChangelogSection } from './gen-update-manifest.mjs'

// Filenames mirror what Tauri 2 + tauri-action upload to GitHub Releases.
// macOS .app.tar.gz has no version in the filename; Linux uses .AppImage
// directly (no .tar.gz wrapper); Windows uses .exe directly (no .nsis.zip).
const sample = [
  { name: 'IEC104Slave_aarch64.app.tar.gz', browser_download_url: 'u1' },
  { name: 'IEC104Slave_aarch64.app.tar.gz.sig', browser_download_url: 'u1s' },
  { name: 'IEC104Slave_1.0.14_x64-setup.exe', browser_download_url: 'u2' },
  { name: 'IEC104Slave_1.0.14_x64-setup.exe.sig', browser_download_url: 'u2s' },
  { name: 'IEC104Master_1.0.14_amd64.AppImage', browser_download_url: 'u3' },
  { name: 'IEC104Master_1.0.14_amd64.AppImage.sig', browser_download_url: 'u3s' },
  // installers that should NOT match (.dmg, .msi, .deb, .rpm) — included to
  // verify the regex doesn't pull them in by accident
  { name: 'IEC104Slave_1.0.14_x64.dmg', browser_download_url: 'noise1' },
  { name: 'IEC104Slave_1.0.14_x64_en-US.msi', browser_download_url: 'noise2' },
  { name: 'IEC104Master_1.0.14_amd64.deb', browser_download_url: 'noise3' },
  { name: 'IEC104Master-1.0.14-1.x86_64.rpm', browser_download_url: 'noise4' },
]

describe('groupAssetsByRole', () => {
  it('separates slave and master assets', () => {
    const { slave, master } = groupAssetsByRole(sample)
    expect(slave['darwin-aarch64'].url).toBe('u1')
    expect(slave['darwin-aarch64'].sigUrl).toBe('u1s')
    expect(slave['windows-x86_64'].url).toBe('u2')
    expect(slave['windows-x86_64'].sigUrl).toBe('u2s')
    expect(master['linux-x86_64'].url).toBe('u3')
    expect(master['linux-x86_64'].sigUrl).toBe('u3s')
  })
  it('ignores non-updater installers (.dmg/.msi/.deb/.rpm)', () => {
    const { slave, master } = groupAssetsByRole(sample)
    const allUrls = Object.values(slave).concat(Object.values(master)).map((v) => v.url)
    expect(allUrls).not.toContain('noise1')
    expect(allUrls).not.toContain('noise2')
    expect(allUrls).not.toContain('noise3')
    expect(allUrls).not.toContain('noise4')
  })
})

describe('extractChangelogSection', () => {
  const md = `# Changelog\n\n## 1.0.9\n- foo\n- bar\n\n## 1.0.8\n- old\n`
  it('extracts the section for the given version', () => {
    expect(extractChangelogSection(md, '1.0.9')).toBe('- foo\n- bar')
  })
  it('returns empty string when version not found', () => {
    expect(extractChangelogSection(md, '9.9.9')).toBe('')
  })
  it('does not match a version that is a prefix of another', () => {
    const md2 = `## 1.0.10\n- new\n\n## 1.0.1\n- old\n`
    expect(extractChangelogSection(md2, '1.0.1')).toBe('- old')
    expect(extractChangelogSection(md2, '1.0.10')).toBe('- new')
  })
  it('handles the Keep-a-Changelog bracket style `## [1.2.3] - date`', () => {
    const md3 = `## [1.2.3] - 2026-04-28\n- new\n\n## [1.2.2] - 2026-04-27\n- old\n`
    expect(extractChangelogSection(md3, '1.2.3')).toBe('- new')
    expect(extractChangelogSection(md3, '1.2.2')).toBe('- old')
  })
})
