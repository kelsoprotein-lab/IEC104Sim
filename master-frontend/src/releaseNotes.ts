export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator'
export const RELEASES_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '里程碑: IEC 60870-5-104 协议参数全面可配 + 真正的 t1/t2/t3/k/w 链路层状态机 — 连接对话框新增 "协议参数" 折叠区可填 t0/t1/t2/t3/k/w、默认 QOI/QCC、总召唤/计数量召唤自动周期; 后端按规范实现 k 阻塞发送、w 强制 ACK、t2 延迟 ACK、t3 TESTFR ACT、t1 超时关连接',
  '新增: 周期性总召唤 / 计数量召唤后台任务, 周期由 interrogate_period_s / counter_interrogate_period_s 控制, 0 表示关闭',
  '新增: 工具栏 "检查更新" 按钮, 绕过 6h 节流和 24h snooze; 修复用户装新版后 6h 内重启错过下一版的体验缺陷',
  '改进: 连接对话框字段在 localStorage 持久化, 编辑模式从后端回填; 旧 v1 表单自动迁移',
]
