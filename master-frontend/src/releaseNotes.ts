export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator'
export const RELEASES_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '通信日志大改版: 帧类型与时间格式跟随中英文切换; 新增 "传送原因 (COT)" 列, 把 COT=3 显示为 "突发" 等可读名称; 顶部拖拽手柄可调整面板高度, 持久化到 localStorage; 最新条目自动置顶',
  '修复: TLS 模式下点击 "断开" 或发送命令延迟数秒 — 接收循环改非阻塞, 共享 mutex 不再被阻塞读卡死, 命令发送最坏延迟降到 ~5 ms',
  '修复: 15k+ 数据点的连接点击 "删除" UI 冻结 1–2 秒 — 后端短锁 + tokio::spawn 异步析构; selectedPoints 改 shallowRef + refreshTree 80 ms 防抖',
  '改进: 全应用统一暗色滚动条, 覆盖 macOS "始终显示滚动条" 模式下的白色 track',
]
