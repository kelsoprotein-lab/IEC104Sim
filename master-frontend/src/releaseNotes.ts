export const APP_NAME = 'IEC104 Master'
export const REPO_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator'
export const RELEASES_URL = 'https://github.com/Carl-Dai/IEC60870-5-104-Simulator/releases'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  '新增 "报文解析器" 工具: 顶栏点开后粘贴一段 hex APDU, 即刻得到 APCI / ASDU / IOA 三段式可视化, 覆盖 25 种 ASDU 类型 (监视方向 NA + 时标 TB/TD/TE/TF + 控制命令 + 系统命令)',
  '通信日志条目右键即可 "解析此报文", 自动用该条 raw_bytes 填充, 无需复制粘贴',
  '性能: 日志面板未展开时, master 收发热路径整段跳过 format!() 字符串构造 (LogCollector 加 enabled flag + active_lc helper), 大流量场景下 CPU 与堆压力明显下降',
  '修复: master 接收循环 4 处编译错误 (active_lc 漏传引用), workspace 重新可构建, cargo test 65/65 全绿',
  '改进: types.ts 抽出 ChangedCategoriesMap / CategoryCountsMap 别名, 三处组件不再重复嵌套泛型',
]
