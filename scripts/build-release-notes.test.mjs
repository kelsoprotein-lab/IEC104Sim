import { describe, it, expect } from 'vitest'
import { buildBody } from './build-release-notes.mjs'

const md = `# Changelog

## [1.2.3] - 2026-04-28

### Highlights / 亮点

- 新功能 X / Feature X.

## [1.2.2] - 2026-04-27

- old
`

describe('buildBody', () => {
  it('renders the version header', () => {
    expect(buildBody('v1.2.3', md)).toMatch(/^# IEC60870-5-104 Simulator v1\.2\.3\b/)
  })
  it('embeds the matching CHANGELOG section', () => {
    expect(buildBody('v1.2.3', md)).toContain('### Highlights / 亮点')
    expect(buildBody('v1.2.3', md)).toContain('新功能 X')
    // must NOT pull in the older section
    expect(buildBody('v1.2.3', md)).not.toContain('1.2.2')
    expect(buildBody('v1.2.3', md)).not.toContain('old')
  })
  it('includes the per-OS download table for both apps', () => {
    const body = buildBody('v1.2.3', md)
    expect(body).toContain('IEC104Slave_1.2.3_aarch64.dmg')
    expect(body).toContain('IEC104Master_1.2.3_aarch64.dmg')
    expect(body).toContain('IEC104Slave_1.2.3_x64-setup.exe')
    expect(body).toContain('IEC104Master_1.2.3_amd64.AppImage')
    expect(body).toContain('IEC104Slave-1.2.3-1.x86_64.rpm')
  })
  it('warns when the version section is missing', () => {
    expect(buildBody('v9.9.9', md)).toContain('CHANGELOG.md 缺少 `9.9.9`')
  })
  it('keeps the footer with full-changelog and releases links', () => {
    const body = buildBody('v1.2.3', md)
    expect(body).toContain('blob/main/CHANGELOG.md')
    expect(body).toContain('/releases>')
  })
})
