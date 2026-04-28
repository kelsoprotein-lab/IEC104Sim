export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/kelsoprotein-lab/IEC60870-5-104-Simulator'
export const RELEASES_URL = 'https://github.com/kelsoprotein-lab/IEC60870-5-104-Simulator/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '修复: macOS 给 .app 加 ad-hoc 签名, 修 v1.1.1 及之前下载后被判定为 "已损坏" 的问题 (右键 → 打开即可)',
  '新增: 主站数据按公共地址 (CA) 真隔离 — 同一连接下不同站的同 IOA 不再相互覆盖',
  '新增: 多 CA 连接树自动展开为 连接 → CA 徽章 → 分类 三层, 各 CA 计数独立',
  '新增: 工具栏增加 "自定义控制" 按钮, 不必先选数据点; CA 字段下拉选当前连接已配置 CAs',
  '新增: 控制对话框记忆 CA / IOA / 命令类型 / 值 (持久化), 发送成功不再自动关闭, 可连续发命令',
  '改进: 右键控制命令直接用数据点自身的 CA, 多 CA 场景路由正确',
  '修复: TLS 模式下点 "断开" 前端永远停在 Connected 的老 bug',
]
