export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/kelsoprotein-lab/IEC104Sim'
export const RELEASES_URL = 'https://github.com/kelsoprotein-lab/IEC104Sim/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '新增: 新建连接可选择 TLS 版本策略(Auto / 仅 TLS 1.2 / 仅 TLS 1.3)',
  '新增: 新建连接表单自动持久化,下次打开自动回填上次的 TLS 路径与目标地址',
  '改进: 窗口标题精简为 IEC104Master;移除源码中写死的本机绝对路径',
]
