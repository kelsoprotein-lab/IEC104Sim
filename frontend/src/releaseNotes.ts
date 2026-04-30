export const APP_NAME = 'IEC104 Slave'
export const REPO_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator'
export const RELEASES_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '新增 "报文解析器" 工具: 顶栏点开后粘贴一段 hex APDU, 即刻得到 APCI / ASDU / IOA 三段式可视化, 覆盖 25 种 ASDU 类型 (监视方向 NA + 时标 TB/TD/TE/TF + 控制命令 + 系统命令)',
  '通信日志条目右键即可 "解析此报文", 自动用该条 raw_bytes 填充, 无需复制粘贴',
  '后端: 子站新增 parse_frame_full Tauri 命令, 返回结构化 ParsedFrame; 老的 parse_apci 字符串摘要命令保留, 不破坏外部调用',
  '配套: iec104sim-core 新增 decode 模块 (ParsedFrame / ParsedApci / ParsedAsdu / ParsedObject / Cp56Time2a) + 10 项单元测试',
  '上一版 v1.2.1 亮点: 子站 16 ASDU 默认初始化, 同 IOA 多类型共存, 写值/日志卡顿修复',
]
