# 中英文切换 (i18n) 设计

- 日期: 2026-04-27
- 范围: 主端 (`master-frontend`) + 从端 (`frontend`) + 两端对应的 Rust Tauri app crate
- 状态: Draft (待用户审阅)

## 背景与目标

当前主从两个前端的 UI 文案、对话框提示、错误信息、日志面板内容均为硬编码中文（约 6400 行 `.vue` 文件），少量 Rust `commands.rs` 中也包含面向用户的中文字符串（如 `单点命令 IOA={} val={}`）。项目已存在中英文双语 README 与 CHANGELOG 意图，需要在运行时支持中英文切换。

目标：

1. 主从两端均支持中英文动态切换
2. 切换覆盖前端所有 UI 文案 + 前端展示的来自后端的运行时文本
3. 跟随系统语言初始化，用户切换后持久化
4. 切换 UI 简单可见，一键即可
5. 不引入额外依赖，保持现有 Vue 3 + TS + Tauri 2 技术栈

## 范围

### 包含

- 主从两端所有 `.vue` 组件中硬编码中文（按钮、菜单、对话框、表头、提示）
- 前端 LogPanel 中显示的来自 Rust 后端的运行时文本
- 主从两端 Rust 侧 `commands.rs` 中面向用户的中文字符串（改为发结构化事件）
- 默认站名 `站 1` 这类后端生成的展示文本
- LogPanel CSV 导出文本（跟随当前 UI locale）

### 不包含 (YAGNI)

- 复数形式 (项目无此需求)
- 日期/数字本地化格式 (项目使用固定 hex/数字)
- 三种及以上语言 (仅中英两种)
- Tauri 后端 tracing 日志本地化 (开发/运维查看，固定英文即可)
- README / CHANGELOG / 应用元数据本地化 (已存在双语 README，不变更)
- 后端日志文件持久化的语言

## 决策摘要

| 决策点 | 结果 |
| --- | --- |
| 本地化范围 | 前端 UI + 前端展示的后端运行时文本 |
| 默认语言 | 跟随系统语言 (`zh*` → `zh-CN`，否则 `en-US`) |
| 持久化 | `localStorage`，重启后保持 |
| 切换 UI 位置 | 工具栏右侧 (`Toolbar.vue` 标题旁) |
| 切换控件形态 | 紧凑 toggle 按钮 `中 / EN` |
| i18n 方案 | 自研轻量 composable (无新依赖) |
| 后端文本本地化路径 | Rust 侧发结构化事件，前端字典渲染 |
| CSV 导出语言 | 跟随当前 UI locale |

## 架构

主从两端各自独立维护一套 i18n 模块（不抽离共享 npm 包以避免引入 workspace 复杂度）。两端的 composable API 与字典结构完全一致，便于复制移植与日后维护。

```
frontend/src/i18n/                  master-frontend/src/i18n/
  index.ts                            index.ts
  detect.ts                           detect.ts
  locales/                            locales/
    zh-CN.ts                            zh-CN.ts
    en-US.ts                            en-US.ts
```

## Composable 设计

### 公开 API

```ts
import { useI18n } from '@/i18n'

const { t, locale, setLocale } = useI18n()

t('toolbar.newServer')
t('log.singleCommand', { ioa: 100, val: 1 })
locale.value                  // 'zh-CN' | 'en-US'
setLocale('en-US')
```

### 实现要点

- `locale` 为模块级 `ref<'zh-CN' | 'en-US'>`（全局单例，所有组件共享）
- `t(key, params?)` 在当前 `locale` 对应的字典中查 key，对结果中 `{xxx}` 占位符做参数替换
- key 找不到时回退顺序：当前 locale → `en-US` → 原样返回 key（便于发现遗漏）
- `t` 在模板中调用时自动响应 `locale` 变化（依赖 Vue 3 响应式）
- 启动初始化顺序：
  1. `localStorage.getItem('iec104.locale')` 若有效则使用
  2. 否则读取 `navigator.language`，以 `zh` 开头 → `zh-CN`
  3. 否则默认 `en-US`
- `setLocale(next)` 写 `locale.value` 并同步 `localStorage`

### 占位符语法

仅支持简单 `{name}` 替换。无嵌套、无管道、无格式化函数。覆盖项目所有现有需求。

## 字典组织

按组件 / 功能域分 namespace。键名采用 camelCase。两端共有的键（确认/取消/启动/停止等）放在 `common.*` 下。

```ts
// 示例 (zh-CN.ts)
export default {
  common: {
    confirm: '确认',
    cancel: '取消',
    start: '启动',
    stop: '停止',
    ok: '确定',
  },
  toolbar: {
    newServer: '新建服务器',
    addStation: '添加站',
    randomMutation: '随机变化',
    stopMutation: '停止变化',
    cyclic: '周期发送',
    stopCyclic: '停止周期',
    about: '关于',
    appTitleSlave: 'IEC 104 Slave',
    appTitleMaster: 'IEC 104 Master',
  },
  tree: { servers: '服务器', station: '站', /* ... */ },
  table: { ioa: '点号', value: '值', type: '类型', /* ... */ },
  log: {
    singleCommand: '单点命令 IOA={ioa} val={val}',
    doubleCommand: '双点命令 IOA={ioa} val={val}',
    stepCommand: '步调节命令 IOA={ioa} val={val}',
    setpointNormalized: '归一化设定值 IOA={ioa} val={val}',
    setpointScaled: '标度化设定值 IOA={ioa} val={val}',
    setpointFloat: '浮点设定值 IOA={ioa} val={val}',
    /* ... */
  },
  station: {
    defaultName: '站 {ca}',     // 替换 Rust 侧硬编码 "站 1"
  },
  errors: {
    invalidPort: '请输入有效的端口号 (1-65535)',
    invalidCa: '请输入有效的公共地址 (1-65534)',
    /* ... */
  },
  about: { /* ... */ },
}
```

`en-US.ts` 提供等价的英文翻译，键集合与中文严格一致。

### 字典完整性的保证

- TypeScript：`zh-CN.ts` 用 `as const` 导出；`en-US.ts` 类型声明为 `typeof zhCN`，缺键时编译失败
- 这样新增中文键忘记加英文翻译会被 `vue-tsc` 直接拦下

## 切换 UI

### 位置

`Toolbar.vue` 右侧，紧邻应用标题按钮 (`IEC 104 Slave` / `IEC 104 Master`) 的左侧。

### 形态

紧凑 toggle 按钮：

```
[ 中 | EN ]
```

- 当前 locale 一侧高亮 (与现有 toolbar 风格一致：背景 `#313244` 等)
- 点击切换到另一侧（仅两种语言时 toggle 优于下拉，少一次点击）
- 与现有 `toolbar-btn` 风格保持一致，使用现有 CSS 变量颜色

### 行为

- 点击立即切换 `locale` ref → 全 UI 通过 `t()` 响应式更新
- 同步写 `localStorage`
- 已显示的日志条目（来自后端结构化事件）也立即重渲染为新语言

## 后端事件本地化

### 现状

Rust 侧 `commands.rs` 中存在面向用户的中文 `format!`，例如：

```rust
&format!("单点命令 IOA={} val={}", ioa, value)
&format!("双点命令 IOA={} val={}", ioa, value)
// ... 共约 6 处在 master 端，1 处在 slave 端 (默认站名 "站 1")
```

### 重构

将这些 `format!` 替换为发结构化事件。事件 schema：

```rust
pub struct LogEvent {
    pub kind: String,        // 例如 "single_command"
    pub payload: serde_json::Value,
    pub timestamp: i64,
    // ...其它已有字段
}
```

`commands.rs` 中：

```rust
// before
log_to_panel(format!("单点命令 IOA={} val={}", ioa, value));

// after
emit_log_event("single_command", json!({ "ioa": ioa, "val": value }));
```

前端 `LogPanel` 渲染：

```ts
const text = t(`log.${event.kind}`, event.payload)
```

### 默认站名

Rust 侧不再硬编码字符串 `站 1`。改为发送 CA 数值，前端展示时若 station 名称为空则用 `t('station.defaultName', { ca })` 生成显示名。

或者 Rust 直接保留固定英文 `Station {ca}`（仅作为内部标识），前端展示时若识别到该格式则替换为本地化字符串。设计倾向前者（语义更清晰）；具体在实现计划中再敲定。

### 兼容性

后端结构化事件的 `kind` 列表收敛在文档中，新增 kind 时需同步加字典。前端遇到未知 kind 时回退展示 `kind + payload JSON` 字符串，便于排查。

## CSV 导出

LogPanel 的 CSV 导出基于已渲染的文本字段，因此自然跟随当前 UI locale。无额外工作。

## 测试

### 单元 (Vitest，需新增)

- `detect.ts`：mock `navigator.language`，断言返回正确的 locale
- `useI18n.ts`：
  - `t(key)` 返回当前 locale 的字符串
  - `t(key, params)` 正确替换 `{xxx}` 占位符
  - 找不到 key 时回退到 en-US，再找不到返回 key
  - `setLocale` 写 localStorage 并触发响应式更新
  - 初始化顺序：localStorage > 系统语言 > 默认

### 类型测试

- `en-US.ts` 缺键时 `vue-tsc` 报错（构建时验证）

### 集成 / 手工

- 启动主端 dev (`pnpm --dir master-frontend dev`)，遍历：
  - 工具栏所有按钮 title / label
  - 新建连接对话框、控制对话框、AboutDialog
  - 错误提示（端口非法、CA 非法等）
  - 日志面板：触发各类控制命令，确认日志条目随切换语言变化
  - CSV 导出，确认导出文件语言匹配当前 locale
- 启动从端 dev (`pnpm --dir frontend dev`)，遍历对应组件
- 切换语言后重启应用，确认 locale 持久化生效
- 在英文系统 locale 下首次启动 → 默认英文
- 在中文系统 locale 下首次启动 → 默认中文

## 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| 漏掉硬编码中文字符串 | 规划阶段对每个 `.vue` 文件 grep `[一-龥]`，逐文件清零；CI 可加 grep 检查作为回归保护 |
| 主从字典漂移 (键集合不一致) | 通过 TypeScript `as const` 类型约束，缺键编译失败 |
| 后端事件 `kind` 与字典脱节 | 字典 + Rust 常量集中维护清单，code review 时核对 |
| LogPanel 已显示日志切换语言后未刷新 | 渲染时直接调用 `t(kind, payload)`，依赖 Vue 响应式自动重算 |
| 文本变长破坏布局 (英文通常比中文长 1.5x) | 实现后手工巡检每个工具栏 / 对话框，必要时调整 min-width / 截断 |

## 实施顺序建议 (供 writing-plans 参考)

1. 主端 (`master-frontend`) i18n 基础设施：composable + 字典骨架 + 工具栏切换按钮
2. 主端组件文案迁移：Toolbar → ConnectionTree → DataTable → ValuePanel → LogPanel → ControlDialog → AboutDialog → AppDialog
3. 主端 Rust (`iec104master-app/src/commands.rs`) 结构化日志事件改造
4. 主端 LogPanel 适配结构化事件 + 端到端验证
5. 复制 i18n 模块到从端 (`frontend`)，重复 2-4 步
6. 从端 Rust (`iec104sim-app/src/commands.rs`) 默认站名处理
7. 类型测试 + 单元测试
8. 手工 E2E 巡检中英文双向切换 + 持久化 + 系统语言检测
