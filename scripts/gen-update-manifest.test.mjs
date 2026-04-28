import { describe, it, expect } from 'vitest'
import { groupAssetsByRole, extractChangelogSection } from './gen-update-manifest.mjs'

const sample = [
  { name: 'IEC104Slave_1.0.9_aarch64.app.tar.gz', browser_download_url: 'u1' },
  { name: 'IEC104Slave_1.0.9_aarch64.app.tar.gz.sig', browser_download_url: 'u1s' },
  { name: 'IEC104Slave_1.0.9_x64-setup.nsis.zip', browser_download_url: 'u2' },
  { name: 'IEC104Slave_1.0.9_x64-setup.nsis.zip.sig', browser_download_url: 'u2s' },
  { name: 'IEC104Master_1.0.9_amd64.AppImage.tar.gz', browser_download_url: 'u3' },
  { name: 'IEC104Master_1.0.9_amd64.AppImage.tar.gz.sig', browser_download_url: 'u3s' },
]

describe('groupAssetsByRole', () => {
  it('separates slave and master assets', () => {
    const { slave, master } = groupAssetsByRole(sample)
    expect(slave['darwin-aarch64'].url).toBe('u1')
    expect(slave['darwin-aarch64'].sigUrl).toBe('u1s')
    expect(slave['windows-x86_64'].url).toBe('u2')
    expect(master['linux-x86_64'].url).toBe('u3')
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
})
