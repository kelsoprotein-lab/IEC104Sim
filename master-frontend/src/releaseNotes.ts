export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator'
export const RELEASES_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '本次版本主要更新子站 (默认 16 ASDU 类型 + 同 IOA 多类型共存 + 写值/日志卡顿修复), 主站随版本号同步发布, 无功能改动',
  '上一版 v1.2.0 亮点: IEC 60870-5-104 协议参数全面可配 + 真正的 t1/t2/t3/k/w 链路层状态机',
  '上一版 v1.2.0 亮点: 工具栏 "检查更新" 按钮, 绕过 6h 节流和 24h snooze, 修复装新版后短期重启错过下一版的盲区',
]
