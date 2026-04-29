export const APP_NAME = 'IEC104 Slave'
export const REPO_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator'
export const RELEASES_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '子站完整开箱体验大整顿: 新建服务器对话框直接暴露 "每类点数" 输入 (默认 10), 默认初始化覆盖全部 16 个监视方向 ASDU 类型 (NA + 带时标 TB/TD/TE/TF), 不同类型共享同段 IOA 1..N',
  '工具栏启动/停止按钮自动同步后端状态 (订阅 server-state-changed 事件), 树右键启停或后端自动停服都能正确反映',
  '修复: 修改值后 UI 卡顿 (32 万点站点下立即触发全量轮询的卡顿) — 写值改乐观更新',
  '修复: 通信日志每 2 秒整体闪烁 (deep ref 重建) — logs 改 shallowRef + 倒序 + 增量检测',
  '修复: 同 IOA 上有多种 ASDU 类型时, 写值会路由错点报 "unsupported value type" — selectedPoints 全程携带 asdu_type, control 查找优先 NA 变体',
  '修复: 突发上送 (COT=3) 之前默默发到 socket, 通信日志看不到 — 现在每批都记一行 tx 日志',
  '改进: 子站窗口标题简化为 "IEC104Slave"; 添加点对话框补齐 16 种 ASDU 类型; parse_asdu_type 接受 PascalCase / snake_case / 大写下划线三种命名',
]
