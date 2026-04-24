export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/kelsoprotein-lab/IEC104Sim'
export const RELEASES_URL = 'https://github.com/kelsoprotein-lab/IEC104Sim/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '修复: 共用同一 IOA 的浮点与累计量互相覆盖,导致总召唤/累计量召唤后历史值消失',
  '修复: 多连接场景下,一个连接的召唤数据会串扰到其他(已断开)连接的树节点',
]
