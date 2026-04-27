export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/kelsoprotein-lab/IEC104Sim'
export const RELEASES_URL = 'https://github.com/kelsoprotein-lab/IEC104Sim/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '新增: 点击"连接"成功后自动发送一次总召唤,无需手动再点',
  '改进: 新建连接的 TLS 证书路径默认填入 ./ca.pem / ./client.pem / ./client-key.pem,开箱即用',
]
