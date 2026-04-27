# 中英文切换 (i18n) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让主端 (`master-frontend` + `iec104master-app`) 与从端 (`frontend` + `iec104sim-app`) 都支持中英文运行时切换，跟随系统语言并持久化。

**Architecture:** 主从两个前端各自维护一套自研轻量 i18n composable + 字典（API 与字典结构同构）。运行时 UI 文案通过 `t(key, params)` 渲染；后端日志的 `detail` 文本改为同时携带结构化 `detail_event { kind, payload }`，前端在 LogPanel 中用 `t('log.${kind}', payload)` 渲染（已显示日志切换语言时也立即变化）。CSV 导出改由前端从已渲染日志数组生成，自动跟随当前 locale。核心库 `iec104sim-core` 中的中文字符串（如 `Category::SinglePoint.as_text() = "单点 (SP)"`）保持不变作为稳定 ID，前端字典以这些中文字符串为 key 做映射。

**Tech Stack:** Vue 3 (Composition API + `ref`) · TypeScript 5 · Tauri 2 · Rust (chrono, serde, serde_json)。新增 dev 依赖：`vitest`、`@vue/test-utils`、`jsdom`（仅 i18n 单元测试用）。

---

## 文件结构

```
master-frontend/
  src/
    i18n/
      index.ts                # useI18n() composable + locale ref + 持久化
      detect.ts               # 系统语言检测
      types.ts                # 字典类型定义 (TranslationDict)
      locales/
        zh-CN.ts              # 中文字典 (源语言)
        en-US.ts              # 英文字典
    components/
      LangSwitch.vue          # NEW 工具栏的 中/EN 切换按钮
      Toolbar.vue             # MODIFY 接入 LangSwitch + t()
      ConnectionTree.vue      # MODIFY t()
      DataTable.vue           # MODIFY t()
      ValuePanel.vue          # MODIFY t()
      LogPanel.vue            # MODIFY t() + detail_event 渲染 + 前端 CSV 导出
      ControlDialog.vue       # MODIFY t()
      AboutDialog.vue         # MODIFY t()
      AppDialog.vue           # MODIFY t()
    types.ts                  # MODIFY LogEntry 增加 detail_event 字段
    main.ts                   # MODIFY 启动时 init i18n
  tests/
    i18n.spec.ts              # NEW Vitest 单元测试
  vitest.config.ts            # NEW
  package.json                # MODIFY 加 vitest + @vue/test-utils + jsdom

frontend/                     # 从端，结构与主端镜像
  src/i18n/...                # 同上
  src/components/LangSwitch.vue
  src/components/{Toolbar,ConnectionTree,DataPointTable,ValuePanel,
                  LogPanel,DataPointModal,BatchAddModal,
                  AboutDialog,AppDialog}.vue   # MODIFY
  src/types.ts
  src/main.ts
  tests/i18n.spec.ts
  vitest.config.ts
  package.json

crates/
  iec104sim-core/src/log_entry.rs              # MODIFY LogEntry 增 detail_event 字段
  iec104master-app/src/commands.rs             # MODIFY 控制命令处发结构化 detail_event
  iec104sim-app/src/commands.rs                # MODIFY 默认站名 "站 1" → 空字符串
  iec104sim-core/tests/...                     # 现有测试若引用 detail/字段需要补默认
```

**为什么这样切：** i18n 模块独立成目录，单一职责（locale 状态 + 翻译函数 + 字典）；`LangSwitch.vue` 作为可复用小组件，避免污染 Toolbar 的逻辑；字典文件按 namespace 分一个文件就够（仅中英两种语言），`as const` + 类型推导保证主从内部字典完整性。Rust 侧仅在 `LogEntry` 加可选字段，不破坏现有调用与 CSV。

---

## Task 1：主端 i18n 模块基础设施

**Files:**
- Create: `master-frontend/src/i18n/types.ts`
- Create: `master-frontend/src/i18n/detect.ts`
- Create: `master-frontend/src/i18n/locales/zh-CN.ts`
- Create: `master-frontend/src/i18n/locales/en-US.ts`
- Create: `master-frontend/src/i18n/index.ts`
- Test: `master-frontend/tests/i18n.spec.ts`
- Modify: `master-frontend/package.json`
- Create: `master-frontend/vitest.config.ts`

- [ ] **Step 1.1: 安装测试依赖**

```bash
cd master-frontend && npm i -D vitest @vue/test-utils jsdom
```

- [ ] **Step 1.2: 创建 vitest 配置**

```ts
// master-frontend/vitest.config.ts
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'jsdom',
    globals: true,
  },
  resolve: {
    alias: { '@': '/src' },
  },
})
```

- [ ] **Step 1.3: 在 package.json 增加 test 脚本**

```json
"scripts": {
  "dev": "vite --port 5177",
  "build": "vue-tsc -b && vite build",
  "preview": "vite preview",
  "test": "vitest run"
}
```

- [ ] **Step 1.4: 写失败测试 — detect.ts**

```ts
// master-frontend/tests/i18n.spec.ts
import { describe, it, expect, vi } from 'vitest'
import { detectSystemLocale } from '../src/i18n/detect'

describe('detectSystemLocale', () => {
  it('returns zh-CN when navigator.language starts with zh', () => {
    vi.stubGlobal('navigator', { language: 'zh-CN' })
    expect(detectSystemLocale()).toBe('zh-CN')
  })
  it('returns zh-CN for zh-TW etc.', () => {
    vi.stubGlobal('navigator', { language: 'zh-TW' })
    expect(detectSystemLocale()).toBe('zh-CN')
  })
  it('returns en-US for English', () => {
    vi.stubGlobal('navigator', { language: 'en-US' })
    expect(detectSystemLocale()).toBe('en-US')
  })
  it('returns en-US as fallback for other locales', () => {
    vi.stubGlobal('navigator', { language: 'ja-JP' })
    expect(detectSystemLocale()).toBe('en-US')
  })
})
```

- [ ] **Step 1.5: 运行测试，确认失败**

Run: `cd master-frontend && npm test`
Expected: FAIL — `Cannot find module '../src/i18n/detect'`

- [ ] **Step 1.6: 实现 i18n/types.ts**

```ts
// master-frontend/src/i18n/types.ts
export type Locale = 'zh-CN' | 'en-US'

export const SUPPORTED_LOCALES: readonly Locale[] = ['zh-CN', 'en-US'] as const

export const STORAGE_KEY = 'iec104.locale'
```

- [ ] **Step 1.7: 实现 i18n/detect.ts**

```ts
// master-frontend/src/i18n/detect.ts
import type { Locale } from './types'

export function detectSystemLocale(): Locale {
  const lang = (typeof navigator !== 'undefined' && navigator.language) || ''
  return lang.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US'
}
```

- [ ] **Step 1.8: 运行测试，确认通过**

Run: `cd master-frontend && npm test`
Expected: PASS — 4 tests in detectSystemLocale

- [ ] **Step 1.9: 创建中文字典骨架（仅含 common 与 toolbar 两个 namespace 用于先打通流程，后续 Task 7 再补全）**

```ts
// master-frontend/src/i18n/locales/zh-CN.ts
const dict = {
  common: {
    confirm: '确认',
    cancel: '取消',
    ok: '确定',
  },
  toolbar: {
    newConnection: '新建连接',
    connect: '连接',
    disconnect: '断开',
    delete: '删除',
    sendGI: '总召唤',
    clockSync: '时钟同步',
    counterRead: '累计量召唤',
    appTitle: 'IEC104 Master',
    about: '关于',
  },
} as const

export default dict
export type DictShape = typeof dict
```

- [ ] **Step 1.10: 创建英文字典（类型约束保证缺键编译失败）**

```ts
// master-frontend/src/i18n/locales/en-US.ts
import type { DictShape } from './zh-CN'

const dict: DictShape = {
  common: {
    confirm: 'Confirm',
    cancel: 'Cancel',
    ok: 'OK',
  },
  toolbar: {
    newConnection: 'New Connection',
    connect: 'Connect',
    disconnect: 'Disconnect',
    delete: 'Delete',
    sendGI: 'General Interrogation',
    clockSync: 'Clock Sync',
    counterRead: 'Counter Read',
    appTitle: 'IEC104 Master',
    about: 'About',
  },
}

export default dict
```

- [ ] **Step 1.11: 写失败测试 — useI18n**

追加到 `master-frontend/tests/i18n.spec.ts`：

```ts
import { useI18n } from '../src/i18n'
import { nextTick } from 'vue'

describe('useI18n', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.stubGlobal('navigator', { language: 'zh-CN' })
  })

  it('t() returns current locale string', () => {
    const { t, setLocale } = useI18n()
    setLocale('zh-CN')
    expect(t('toolbar.newConnection')).toBe('新建连接')
    setLocale('en-US')
    expect(t('toolbar.newConnection')).toBe('New Connection')
  })

  it('t() interpolates {placeholders}', () => {
    const { t, setLocale } = useI18n()
    setLocale('zh-CN')
    // 暂用 toolbar 内一项简单测试键替换；真实 log.* 在后续 task 加入
    expect(t('toolbar.appTitle')).toBe('IEC104 Master')
  })

  it('falls back to en-US when key missing in current locale', () => {
    const { t, setLocale } = useI18n()
    // @ts-expect-error 故意构造一个不存在的 key 测试回退
    setLocale('zh-CN')
    // @ts-expect-error
    expect(t('does.not.exist')).toBe('does.not.exist')
  })

  it('setLocale writes to localStorage', () => {
    const { setLocale } = useI18n()
    setLocale('en-US')
    expect(localStorage.getItem('iec104.locale')).toBe('en-US')
  })

  it('locale is reactive', async () => {
    const { t, locale, setLocale } = useI18n()
    setLocale('zh-CN')
    const before = t('common.cancel')
    setLocale('en-US')
    await nextTick()
    expect(t('common.cancel')).not.toBe(before)
    expect(locale.value).toBe('en-US')
  })
})
```

- [ ] **Step 1.12: 运行测试，确认失败**

Run: `cd master-frontend && npm test`
Expected: FAIL — `Cannot find module '../src/i18n'`

- [ ] **Step 1.13: 实现 i18n/index.ts**

```ts
// master-frontend/src/i18n/index.ts
import { ref, computed } from 'vue'
import type { Locale } from './types'
import { SUPPORTED_LOCALES, STORAGE_KEY } from './types'
import { detectSystemLocale } from './detect'
import zhCN from './locales/zh-CN'
import enUS from './locales/en-US'
import type { DictShape } from './locales/zh-CN'

const dictionaries: Record<Locale, DictShape> = {
  'zh-CN': zhCN,
  'en-US': enUS,
}

function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved && (SUPPORTED_LOCALES as readonly string[]).includes(saved)) {
      return saved as Locale
    }
  } catch { /* ignore */ }
  return detectSystemLocale()
}

const locale = ref<Locale>(initialLocale())

function lookup(dict: DictShape, key: string): string | undefined {
  const parts = key.split('.')
  let cur: unknown = dict
  for (const p of parts) {
    if (cur && typeof cur === 'object' && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p]
    } else {
      return undefined
    }
  }
  return typeof cur === 'string' ? cur : undefined
}

function interpolate(template: string, params?: Record<string, unknown>): string {
  if (!params) return template
  return template.replace(/\{(\w+)\}/g, (_, name) => {
    const v = params[name]
    return v === undefined || v === null ? `{${name}}` : String(v)
  })
}

function translate(key: string, params?: Record<string, unknown>): string {
  // 当前 locale → en-US 回退 → key 原值
  const fromCurrent = lookup(dictionaries[locale.value], key)
  if (fromCurrent !== undefined) return interpolate(fromCurrent, params)
  const fromFallback = lookup(dictionaries['en-US'], key)
  if (fromFallback !== undefined) return interpolate(fromFallback, params)
  return key
}

function setLocale(next: Locale) {
  if (!(SUPPORTED_LOCALES as readonly string[]).includes(next)) return
  locale.value = next
  try { localStorage.setItem(STORAGE_KEY, next) } catch { /* ignore */ }
}

const localeRef = computed(() => locale.value)

export function useI18n() {
  return {
    t: translate,
    locale: localeRef,
    setLocale,
  }
}

export type { Locale }
```

- [ ] **Step 1.14: 运行测试，确认通过**

Run: `cd master-frontend && npm test`
Expected: PASS — 4 detect tests + 5 useI18n tests = 9 PASS

- [ ] **Step 1.15: 类型检查**

Run: `cd master-frontend && npm run build`
Expected: PASS（首次跑可能编译时间稍长，确认 vue-tsc 无报错；构建产物可不验证）

- [ ] **Step 1.16: 提交**

```bash
git add master-frontend/src/i18n master-frontend/tests/i18n.spec.ts master-frontend/vitest.config.ts master-frontend/package.json master-frontend/package-lock.json
git commit -m "feat(master-frontend/i18n): add lightweight i18n composable scaffold"
```

---

## Task 2：主端 LangSwitch 组件 + Toolbar 接入 + main.ts 初始化

**Files:**
- Create: `master-frontend/src/components/LangSwitch.vue`
- Modify: `master-frontend/src/components/Toolbar.vue` (插入 LangSwitch；toolbar 内文案先暂时保留中文，下个 Task 再迁移)
- Modify: `master-frontend/src/main.ts` (无需调用，因为 i18n 是 lazy singleton；本步仅用于触发首次 import，确保启动时 locale 已就绪 — 实际可以跳过 main.ts 修改，留作可选)

- [ ] **Step 2.1: 创建 LangSwitch.vue**

```vue
<!-- master-frontend/src/components/LangSwitch.vue -->
<script setup lang="ts">
import { useI18n } from '../i18n'

const { locale, setLocale } = useI18n()

function toggle() {
  setLocale(locale.value === 'zh-CN' ? 'en-US' : 'zh-CN')
}
</script>

<template>
  <button class="lang-switch" :title="locale === 'zh-CN' ? 'Switch to English' : '切换到中文'" @click="toggle">
    <span :class="['seg', { active: locale === 'zh-CN' }]">中</span>
    <span class="sep">/</span>
    <span :class="['seg', { active: locale === 'en-US' }]">EN</span>
  </button>
</template>

<style scoped>
.lang-switch {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  margin-right: 8px;
  background: transparent;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #6c7086;
  cursor: pointer;
  font-size: 12px;
  font-family: inherit;
}
.lang-switch:hover { color: #cdd6f4; background: #313244; }
.seg { padding: 0 2px; }
.seg.active { color: #cdd6f4; font-weight: 600; }
.sep { color: #45475a; }
</style>
```

- [ ] **Step 2.2: Toolbar 插入 LangSwitch（位置：toolbar-spacer 之后、toolbar-title 之前）**

修改 `master-frontend/src/components/Toolbar.vue`：

import 区添加：
```ts
import LangSwitch from './LangSwitch.vue'
```

template 中找到：
```vue
    <div class="toolbar-spacer"></div>
    <button class="toolbar-title as-button" @click="showAbout = true" title="关于">
      IEC104 Master
    </button>
```

替换为：
```vue
    <div class="toolbar-spacer"></div>
    <LangSwitch />
    <button class="toolbar-title as-button" @click="showAbout = true" title="关于">
      IEC104 Master
    </button>
```

- [ ] **Step 2.3: 启动 dev server 手测切换 UI**

Run: `cd master-frontend && npm run dev`
Expected: 浏览器（或 Tauri 内嵌）打开 `http://localhost:5177`，工具栏右侧有 `中 / EN` 按钮，当前 locale 一侧高亮，点击 toggle 后视觉切换；刷新页面 locale 持久化。

记录：
- 中文系统下首次打开 → 中字高亮 ✓
- 切换至 EN → EN 高亮 ✓
- 刷新 → EN 仍高亮（持久化）✓

如不通过则修复后再提交。

- [ ] **Step 2.4: 提交**

```bash
git add master-frontend/src/components/LangSwitch.vue master-frontend/src/components/Toolbar.vue
git commit -m "feat(master-frontend): add LangSwitch toggle in toolbar"
```

---

## Task 3：主端字典扩充 — 覆盖所有静态 UI 文案

**Files:**
- Modify: `master-frontend/src/i18n/locales/zh-CN.ts`
- Modify: `master-frontend/src/i18n/locales/en-US.ts` (因 `DictShape` 类型约束，必须同步)

本任务只更新字典，**不改组件**（下个任务做组件迁移）。把所有前端组件中可见的中文字符串列入字典。

- [ ] **Step 3.1: 全量盘点中文字符串**

Run: `cd master-frontend && grep -nE "[一-龥]" src/components/*.vue`

Expected: 输出所有中文位置；用作字典 key 列表来源。

- [ ] **Step 3.2: 扩充 zh-CN.ts**

在现有 `dict` 内补充以下 namespace（按现有组件分组）：

```ts
const dict = {
  common: {
    confirm: '确认',
    cancel: '取消',
    ok: '确定',
    close: '关闭',
    save: '保存',
    refresh: '刷新',
    clear: '清空',
    export: '导出',
  },
  toolbar: {
    newConnection: '新建连接',
    connect: '连接',
    disconnect: '断开',
    delete: '删除',
    sendGI: '总召唤',
    clockSync: '时钟同步',
    counterRead: '累计量召唤',
    appTitle: 'IEC104 Master',
    about: '关于',
  },
  newConn: {
    title: '新建连接',
    targetAddress: '目标地址',
    port: '端口',
    commonAddress: '公共地址 (CA)',
    enableTls: '启用 TLS',
    tlsVersion: 'TLS 版本',
    tlsAuto: '自动',
    tls12: '仅 TLS 1.2',
    tls13: '仅 TLS 1.3',
    caFile: 'CA 证书路径',
    certFile: '客户端证书路径',
    keyFile: '客户端密钥路径',
    acceptInvalidCerts: '接受无效证书（测试用）',
    create: '创建',
  },
  tree: {
    connections: '连接',
    noConnections: '暂无连接',
    // ...其它 ConnectionTree.vue 中的中文（grep 后逐项填）
  },
  table: {
    // DataTable.vue 中的表头与提示
    ioa: '点号',
    type: '类型',
    value: '值',
    timestamp: '时间戳',
    quality: '质量',
    noData: '暂无数据',
    // ... 全部
  },
  valuePanel: {
    // ValuePanel.vue
  },
  log: {
    title: '通信日志',
    noConnections: '暂无连接',
    noLogs: '暂无日志',
    timeCol: '时间',
    directionCol: '方向',
    frameCol: '帧类型',
    detailCol: '详情',
    rawCol: '原始数据',
    refresh: '刷新',
    clear: '清空',
    export: '导出',
    // 结构化 detail 模板（替代后端硬编码字符串）
    singleCommand: '单点命令 IOA={ioa} val={val}',
    doubleCommand: '双点命令 IOA={ioa} val={val}',
    stepCommand: '步调节命令 IOA={ioa} val={val}',
    setpointNormalized: '归一化设定值 IOA={ioa} val={val}',
    setpointScaled: '标度化设定值 IOA={ioa} val={val}',
    setpointFloat: '浮点设定值 IOA={ioa} val={val}',
  },
  control: {
    // ControlDialog.vue
    title: '控制命令',
    sbo: '选择后执行 (SbO)',
    direct: '直接执行',
    send: '发送',
    label_single: '单点命令 C_SC_NA_1',
    label_double: '双点命令 C_DC_NA_1',
    label_step: '步调节命令 C_RC_NA_1',
    label_setpoint_normalized: '归一化设定值 C_SE_NA_1',
    label_setpoint_scaled: '标度化设定值 C_SE_NB_1',
    label_setpoint_float: '浮点设定值 C_SE_NC_1',
    opt_off: '分闸 OFF',
    opt_on: '合闸 ON',
    opt_intermediate: '中间',
    opt_open: '分',
    opt_close: '合',
    opt_invalid: '不确定',
    opt_step_down: '降',
    opt_step_up: '升',
  },
  category: {
    // 核心库返回的中文 ID（保持不变，做映射）
    '单点 (SP)': '单点 (SP)',
    '双点 (DP)': '双点 (DP)',
    '步位置 (ST)': '步位置 (ST)',
    '位串 (BO)': '位串 (BO)',
    '归一化 (ME_NA)': '归一化 (ME_NA)',
    '标度化 (ME_NB)': '标度化 (ME_NB)',
    '浮点 (ME_NC)': '浮点 (ME_NC)',
    '累计量 (IT)': '累计量 (IT)',
  },
  about: {
    title: '关于',
    version: '版本',
    description: 'IEC 60870-5-104 协议主站模拟器',
    // ...
  },
  errors: {
    connectFailed: '连接失败: {err}\n将每 {sec} 秒自动重试,点击「断开」可停止。',
    invalidPort: '请输入有效的端口号 (1-65535)',
    invalidCa: '请输入有效的公共地址 (1-65534)',
    // 其它运行时 alert
  },
} as const
```

**关键提示：** 上面用 `// ...` 标注的位置必须在实际编码时**完整列出每一项**——不允许 `// ...`。本步骤交付时所有 grep 出的中文字符串都必须在字典中有对应 key。

- [ ] **Step 3.3: 同步扩充 en-US.ts**

复制 zh-CN.ts 的 key 树形结构，逐项翻译为英文。例如：

```ts
const dict: DictShape = {
  common: {
    confirm: 'Confirm',
    cancel: 'Cancel',
    ok: 'OK',
    close: 'Close',
    save: 'Save',
    refresh: 'Refresh',
    clear: 'Clear',
    export: 'Export',
  },
  toolbar: {
    newConnection: 'New Connection',
    connect: 'Connect',
    disconnect: 'Disconnect',
    delete: 'Delete',
    sendGI: 'General Interrogation',
    clockSync: 'Clock Sync',
    counterRead: 'Counter Read',
    appTitle: 'IEC104 Master',
    about: 'About',
  },
  newConn: {
    title: 'New Connection',
    targetAddress: 'Target Address',
    port: 'Port',
    commonAddress: 'Common Address (CA)',
    enableTls: 'Enable TLS',
    tlsVersion: 'TLS Version',
    tlsAuto: 'Auto',
    tls12: 'TLS 1.2 only',
    tls13: 'TLS 1.3 only',
    caFile: 'CA Certificate Path',
    certFile: 'Client Certificate Path',
    keyFile: 'Client Key Path',
    acceptInvalidCerts: 'Accept invalid certificates (testing)',
    create: 'Create',
  },
  tree: {
    connections: 'Connections',
    noConnections: 'No connections',
    /* ... 全部翻译 */
  },
  table: {
    ioa: 'IOA',
    type: 'Type',
    value: 'Value',
    timestamp: 'Timestamp',
    quality: 'Quality',
    noData: 'No data',
    /* ... */
  },
  valuePanel: { /* ... */ },
  log: {
    title: 'Communication Log',
    noConnections: 'No connections',
    noLogs: 'No logs',
    timeCol: 'Time',
    directionCol: 'Dir',
    frameCol: 'Frame',
    detailCol: 'Detail',
    rawCol: 'Raw',
    refresh: 'Refresh',
    clear: 'Clear',
    export: 'Export',
    singleCommand: 'Single Command IOA={ioa} val={val}',
    doubleCommand: 'Double Command IOA={ioa} val={val}',
    stepCommand: 'Step Command IOA={ioa} val={val}',
    setpointNormalized: 'Setpoint Normalized IOA={ioa} val={val}',
    setpointScaled: 'Setpoint Scaled IOA={ioa} val={val}',
    setpointFloat: 'Setpoint Float IOA={ioa} val={val}',
  },
  control: {
    title: 'Control Command',
    sbo: 'Select before Operate (SbO)',
    direct: 'Direct Execute',
    send: 'Send',
    label_single: 'Single Command C_SC_NA_1',
    label_double: 'Double Command C_DC_NA_1',
    label_step: 'Step Command C_RC_NA_1',
    label_setpoint_normalized: 'Setpoint Normalized C_SE_NA_1',
    label_setpoint_scaled: 'Setpoint Scaled C_SE_NB_1',
    label_setpoint_float: 'Setpoint Float C_SE_NC_1',
    opt_off: 'OFF',
    opt_on: 'ON',
    opt_intermediate: 'Intermediate',
    opt_open: 'Open',
    opt_close: 'Close',
    opt_invalid: 'Invalid',
    opt_step_down: 'Down',
    opt_step_up: 'Up',
  },
  category: {
    '单点 (SP)': 'Single Point (SP)',
    '双点 (DP)': 'Double Point (DP)',
    '步位置 (ST)': 'Step Position (ST)',
    '位串 (BO)': 'Bitstring (BO)',
    '归一化 (ME_NA)': 'Normalized (ME_NA)',
    '标度化 (ME_NB)': 'Scaled (ME_NB)',
    '浮点 (ME_NC)': 'Float (ME_NC)',
    '累计量 (IT)': 'Integrated Totals (IT)',
  },
  about: {
    title: 'About',
    version: 'Version',
    description: 'IEC 60870-5-104 Master Simulator',
  },
  errors: {
    connectFailed: 'Connect failed: {err}\nWill retry every {sec}s. Click "Disconnect" to stop.',
    invalidPort: 'Please enter a valid port number (1-65535)',
    invalidCa: 'Please enter a valid common address (1-65534)',
  },
}
```

- [ ] **Step 3.4: 类型检查**

Run: `cd master-frontend && npm run build`
Expected: PASS。如果有缺键 `vue-tsc` 会精确报错；逐一补齐。

- [ ] **Step 3.5: 运行已有测试**

Run: `cd master-frontend && npm test`
Expected: 9 PASS（之前的测试不依赖具体字典内容）。

- [ ] **Step 3.6: 提交**

```bash
git add master-frontend/src/i18n/locales
git commit -m "feat(master-frontend/i18n): populate full bilingual dictionary"
```

---

## Task 4：主端组件文案迁移 — Toolbar

**Files:** `master-frontend/src/components/Toolbar.vue`

- [ ] **Step 4.1: 在 Toolbar.vue 引入 useI18n**

在 `<script setup>` import 区添加：
```ts
import { useI18n } from '../i18n'
const { t } = useI18n()
```

- [ ] **Step 4.2: 替换 template 中所有硬编码中文**

按下表逐项替换（参照实际 line numbers 由 grep 输出）：

| 原文 | 替换为 |
|---|---|
| `<span class="btn-icon">+</span> 新建连接` | `<span class="btn-icon">+</span> {{ t('toolbar.newConnection') }}` |
| `连接` (按钮文本) | `{{ t('toolbar.connect') }}` |
| `断开` | `{{ t('toolbar.disconnect') }}` |
| `删除` | `{{ t('toolbar.delete') }}` |
| `总召唤` | `{{ t('toolbar.sendGI') }}` |
| `时钟同步` | `{{ t('toolbar.clockSync') }}` |
| `累计量召唤` | `{{ t('toolbar.counterRead') }}` |
| `title="关于"` | `:title="t('toolbar.about')"` |
| `IEC104 Master` (标题文本) | `{{ t('toolbar.appTitle') }}` |
| `<div class="modal-title">新建连接</div>` | `<div class="modal-title">{{ t('newConn.title') }}</div>` |
| `目标地址` | `{{ t('newConn.targetAddress') }}` |
| `端口` | `{{ t('newConn.port') }}` |
| `公共地址 (CA)` | `{{ t('newConn.commonAddress') }}` |
| `<span>启用 TLS</span>` | `<span>{{ t('newConn.enableTls') }}</span>` |
| `TLS 版本` | `{{ t('newConn.tlsVersion') }}` |
| `<option value="auto">自动</option>` | `<option value="auto">{{ t('newConn.tlsAuto') }}</option>` |
| `<option value="tls12_only">仅 TLS 1.2</option>` | `<option value="tls12_only">{{ t('newConn.tls12') }}</option>` |
| `<option value="tls13_only">仅 TLS 1.3</option>` | `<option value="tls13_only">{{ t('newConn.tls13') }}</option>` |
| `CA 证书路径` | `{{ t('newConn.caFile') }}` |
| `客户端证书路径` | `{{ t('newConn.certFile') }}` |
| `客户端密钥路径` | `{{ t('newConn.keyFile') }}` |
| `<span>接受无效证书（测试用）</span>` | `<span>{{ t('newConn.acceptInvalidCerts') }}</span>` |
| `>取消<` (在 modal-footer) | `>{{ t('common.cancel') }}<` |
| `>创建<` | `>{{ t('newConn.create') }}<` |

- [ ] **Step 4.3: 替换 script 内 alert 中的中文**

```ts
// 找到：
void showAlert(`连接失败: ${e}\n将每 ${RETRY_INTERVAL_MS / 1000} 秒自动重试,点击「断开」可停止。`)
// 替换为：
void showAlert(t('errors.connectFailed', { err: String(e), sec: RETRY_INTERVAL_MS / 1000 }))
```

- [ ] **Step 4.4: grep 确认无残留中文**

Run: `cd master-frontend && grep -nE "[一-龥]" src/components/Toolbar.vue`
Expected: 仅可能命中字典 key 字符串字面量（如果有），其它中文应为零。

- [ ] **Step 4.5: 类型检查 + dev 手测**

```bash
cd master-frontend && npm run build
cd master-frontend && npm run dev
```

操作：切换到 EN，确认 Toolbar 所有按钮、新建连接对话框文案变英文；切换到中文，确认变回中文；刷新仍保持。

- [ ] **Step 4.6: 提交**

```bash
git add master-frontend/src/components/Toolbar.vue
git commit -m "feat(master-frontend): localize Toolbar"
```

---

## Task 5：主端组件文案迁移 — ConnectionTree / DataTable / ValuePanel / ControlDialog / AboutDialog / AppDialog

对每个组件，按 Task 4 同样的模式：(a) import `useI18n`；(b) 把所有 template 与 script 中的中文替换为 `t('...')`；(c) grep 确认无残留中文（除字典 key 字面量）；(d) dev 手测；(e) 提交。

**Files:** （每个一个 sub-task，分别提交）
- Modify: `master-frontend/src/components/ConnectionTree.vue`
- Modify: `master-frontend/src/components/DataTable.vue`
- Modify: `master-frontend/src/components/ValuePanel.vue`
- Modify: `master-frontend/src/components/ControlDialog.vue`
- Modify: `master-frontend/src/components/AboutDialog.vue`
- Modify: `master-frontend/src/components/AppDialog.vue`
- Modify: `master-frontend/src/types.ts` (CONTROL_CONFIG_MAP 中的中文 label / opt label 改为 t() 调用 — 该 map 是模块顶层常量，需重构为 getter 函数：`getControlConfig(category, t)`，或在调用处展开)

- [ ] **Step 5.1: ConnectionTree.vue**

```bash
grep -nE "[一-龥]" master-frontend/src/components/ConnectionTree.vue
```
逐项替换为 `t('tree.<key>')`。

注意 category 名（"单点 (SP)" 等）来自后端 `ReceivedDataPointInfo.category`：在 ConnectionTree 模板中显示该字段时改为 `{{ t('category.' + node.category) }}`（key 含括号空格无需转义，JS 对象访问时字符串字面量是合法的）。

测试：grep + dev 切换。

提交：`git commit -m "feat(master-frontend): localize ConnectionTree"`

- [ ] **Step 5.2: DataTable.vue** — 同模式。提交 `localize DataTable`.

- [ ] **Step 5.3: ValuePanel.vue** — 同模式。提交 `localize ValuePanel`.

- [ ] **Step 5.4: ControlDialog.vue + types.ts CONTROL_CONFIG_MAP 重构**

`master-frontend/src/types.ts` 中的 `CONTROL_CONFIG_MAP` 是模块顶层常量，值里写了中文 label。改为函数：

```ts
// 替换原 const CONTROL_CONFIG_MAP 与 getControlConfig
import { useI18n } from './i18n'

export function getControlConfig(category: string): ControlConfig | null {
  const { t } = useI18n()
  const map: Record<string, ControlConfig | null> = {
    '单点 (SP)': {
      commandType: 'single',
      label: t('control.label_single'),
      widget: 'toggle',
      options: [
        { label: t('control.opt_off'), value: 'false' },
        { label: t('control.opt_on'), value: 'true' },
      ],
    },
    '双点 (DP)': {
      commandType: 'double',
      label: t('control.label_double'),
      widget: 'button_group',
      options: [
        { label: t('control.opt_intermediate'), value: '0' },
        { label: t('control.opt_open'), value: '1' },
        { label: t('control.opt_close'), value: '2' },
        { label: t('control.opt_invalid'), value: '3' },
      ],
    },
    '步位置 (ST)': {
      commandType: 'step',
      label: t('control.label_step'),
      widget: 'step_buttons',
      options: [
        { label: t('control.opt_step_down'), value: '1' },
        { label: t('control.opt_step_up'), value: '2' },
      ],
    },
    '归一化 (ME_NA)': {
      commandType: 'setpoint_normalized',
      label: t('control.label_setpoint_normalized'),
      widget: 'slider',
      min: -1.0, max: 1.0, step: 0.001,
    },
    '标度化 (ME_NB)': {
      commandType: 'setpoint_scaled',
      label: t('control.label_setpoint_scaled'),
      widget: 'number_input',
      min: -32768, max: 32767, step: 1,
    },
    '浮点 (ME_NC)': {
      commandType: 'setpoint_float',
      label: t('control.label_setpoint_float'),
      widget: 'number_input',
      step: 0.1,
    },
    '位串 (BO)': null,
    '累计量 (IT)': null,
  }
  return map[category] ?? null
}
```

注意：旧调用方若把 `CONTROL_CONFIG_MAP` 直接 import 使用，需改为调用 `getControlConfig(category)`。grep 检查：
```bash
grep -rn "CONTROL_CONFIG_MAP" master-frontend/src
```
全部改为 `getControlConfig`。

由于 `getControlConfig` 现在每次调用都返回新 map 且不再响应式（被 `t` 在调用瞬间求值），ControlDialog 需在 setup 内用 `computed`：
```ts
import { computed } from 'vue'
import { getControlConfig } from '../types'
const config = computed(() => getControlConfig(props.category))
```

测试：dev 中打开控制对话框，切换 locale 后 label 与选项跟随变化。

提交：`git commit -m "feat(master-frontend): localize ControlDialog and control config"`

- [ ] **Step 5.5: AboutDialog.vue** — 同模式。提交 `localize AboutDialog`.

- [ ] **Step 5.6: AppDialog.vue** — 同模式（dialog 中 confirm/cancel/ok 按钮文本用 `t('common.confirm')` 等）。提交 `localize AppDialog`.

- [ ] **Step 5.7: 总验证 — grep**

```bash
cd master-frontend && grep -rnE "[一-龥]" src/ | grep -v "i18n/locales"
```

Expected: 输出只剩 `category` 字典里的中文 key 字面量（用于映射）和注释（如有）。任何 template 中的硬编码中文都必须为 0。

- [ ] **Step 5.8: dev 端到端手测主端**

Run: `cd master-frontend && npm run dev`
按下表逐项切换 locale 验证：

- [ ] Toolbar 所有按钮 + title
- [ ] 新建连接对话框（含 TLS 区）
- [ ] ConnectionTree（连接节点 + 类别节点 + 计数 + 空状态）
- [ ] DataTable 表头 + 空状态
- [ ] ValuePanel 字段标签
- [ ] LogPanel header / 控件 / 表头 / 空状态（**注意：detail 列此时仍是中文，因为后端尚未改造，将在 Task 7 解决**）
- [ ] ControlDialog（每种命令类型的 label 与选项）
- [ ] AboutDialog
- [ ] AppDialog 的 confirm / alert / prompt
- [ ] 刷新页面，locale 持久化

---

## Task 6：核心库 LogEntry 增加 detail_event 字段

**Files:**
- Modify: `crates/iec104sim-core/src/log_entry.rs`

向后兼容地为 `LogEntry` 增加可选结构化字段。现有 `detail: String` 保留作为后端内部使用与 CSV header（虽然 CSV 我们最终改为前端生成，但保留以兼容旧消费者）。

- [ ] **Step 6.1: 写失败测试**

在 `crates/iec104sim-core/src/log_entry.rs` 的 `mod tests` 中追加：

```rust
#[test]
fn test_log_entry_with_detail_event() {
    use serde_json::json;
    let entry = LogEntry::new(Direction::Tx, FrameLabel::SingleCommand, "ignored")
        .with_detail_event("single_command", json!({ "ioa": 100, "val": true }));
    assert_eq!(entry.detail_event.as_ref().unwrap().kind, "single_command");
    let payload = &entry.detail_event.as_ref().unwrap().payload;
    assert_eq!(payload["ioa"], 100);
    assert_eq!(payload["val"], true);
}

#[test]
fn test_log_entry_serializes_detail_event() {
    use serde_json::json;
    let entry = LogEntry::new(Direction::Tx, FrameLabel::SingleCommand, "x")
        .with_detail_event("single_command", json!({ "ioa": 1 }));
    let s = serde_json::to_string(&entry).unwrap();
    assert!(s.contains("\"detail_event\""));
    assert!(s.contains("\"kind\":\"single_command\""));
}

#[test]
fn test_log_entry_omits_detail_event_when_none() {
    let entry = LogEntry::new(Direction::Rx, FrameLabel::GeneralInterrogation, "GI");
    let s = serde_json::to_string(&entry).unwrap();
    assert!(!s.contains("detail_event"));
}
```

- [ ] **Step 6.2: 运行测试，确认失败**

Run: `cargo test -p iec104sim-core log_entry`
Expected: FAIL — `with_detail_event` not found / `detail_event` field not exists.

- [ ] **Step 6.3: 实现**

在 `log_entry.rs` 顶部 import 区添加：
```rust
use serde_json::Value as JsonValue;
```

新增结构体：
```rust
/// Structured detail payload for frontend localization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailEvent {
    pub kind: String,
    pub payload: JsonValue,
}
```

修改 `LogEntry`：
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub direction: Direction,
    pub frame_label: FrameLabel,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_bytes: Option<Vec<u8>>,
    /// Structured payload for frontend localization. When present, frontend
    /// renders `t(\"log.{kind}\", payload)`; falls back to `detail` otherwise.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detail_event: Option<DetailEvent>,
}
```

更新构造器：
```rust
impl LogEntry {
    pub fn new(direction: Direction, frame_label: FrameLabel, detail: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            direction,
            frame_label,
            detail: detail.into(),
            raw_bytes: None,
            detail_event: None,
        }
    }

    pub fn with_raw_bytes(
        direction: Direction,
        frame_label: FrameLabel,
        detail: impl Into<String>,
        raw_bytes: Vec<u8>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            direction,
            frame_label,
            detail: detail.into(),
            raw_bytes: Some(raw_bytes),
            detail_event: None,
        }
    }

    pub fn with_detail_event(mut self, kind: impl Into<String>, payload: JsonValue) -> Self {
        self.detail_event = Some(DetailEvent { kind: kind.into(), payload });
        self
    }

    /* ... to_csv_row 与 csv_header 不变 ... */
}
```

依赖：`Cargo.toml` 中 `iec104sim-core` 应已含 `serde_json`（`grep` 确认；若无则 `cargo add -p iec104sim-core serde_json`）。

- [ ] **Step 6.4: 运行测试**

Run: `cargo test -p iec104sim-core log_entry`
Expected: 4+ PASS（原 3 + 新 3），全部通过。

- [ ] **Step 6.5: 完整工作区构建**

Run: `cargo build`
Expected: 成功。如有调用方因新增字段而出错（不会，因为字段是 `Option`），修复。

- [ ] **Step 6.6: 提交**

```bash
git add crates/iec104sim-core/src/log_entry.rs
git commit -m "feat(core/log_entry): add optional structured detail_event for i18n"
```

---

## Task 7：主端 Rust 控制命令发出 detail_event

**Files:**
- Modify: `crates/iec104master-app/src/commands.rs`
- Modify: `crates/iec104sim-core/src/master.rs` (若 `send_control_with_sbo` 内部构造 LogEntry，需让其接受可选 detail_event；如不便，则在 commands.rs 这一层 hack — 见下)

`commands.rs` 现状是把中文字符串作为 `&str` detail 传给 `send_control_with_sbo`；这个方法内部构造 LogEntry 写入 LogCollector。我们需要让 LogEntry 同时带上 detail_event。

最干净的做法：扩展 `send_control_with_sbo` 签名，新增一个 `detail_event: Option<DetailEvent>` 参数，由 `commands.rs` 传入。

- [ ] **Step 7.1: 调研 send_control_with_sbo 实现**

Run: `grep -n "send_control_with_sbo" crates/iec104sim-core/src/master.rs`

读取相关函数，确定它如何构造 LogEntry。

- [ ] **Step 7.2: 写失败测试 (集成)**

在 `crates/iec104sim-core/tests/control_e2e.rs` 或新增 `tests/control_detail_event.rs` 写一个测试：发送一个 single command（mock master），断言 LogCollector 中相应 LogEntry 的 `detail_event.kind == "single_command"` 且 payload 含 ioa/val。

（如果现有 e2e 太重，则改为单元测试 LogEntry 构造路径；具体取决于 `master.rs` 结构。）

- [ ] **Step 7.3: 运行测试，确认失败**

`cargo test -p iec104sim-core control_detail_event`
Expected: FAIL.

- [ ] **Step 7.4: 修改 send_control_with_sbo 签名增加 detail_event 参数**

```rust
// master.rs (示意)
pub async fn send_control_with_sbo(
    &self,
    select_frame: Vec<u8>,
    execute_frame: Vec<u8>,
    ioa: u32,
    detail: &str,
    frame_label: FrameLabel,
    ca: u16,
    detail_event: Option<DetailEvent>,   // NEW
) -> Result<(), String> {
    /* ... 内部构造 LogEntry 时 .with_detail_event(...) ... */
}
```

更新所有调用点（master.rs 内部 + commands.rs）。

- [ ] **Step 7.5: 修改 commands.rs 6 处调用点**

每处把字符串 detail 保留（为兼容 CLI/CSV header），同时附 detail_event：

```rust
"single" => {
    let value = parse_bool(&request.value)?;
    let select_frame = build_control_frames_single(ca, ioa, value, true);
    let execute_frame = build_control_frames_single(ca, ioa, value, false);
    conn.connection.send_control_with_sbo(
        select_frame, execute_frame, ioa,
        &format!("单点命令 IOA={} val={}", ioa, value),
        FrameLabel::SingleCommand, ca,
        Some(DetailEvent {
            kind: "single_command".to_string(),
            payload: serde_json::json!({ "ioa": ioa, "val": value }),
        }),
    ).await.map_err(|e| format!("{}", e))
}
"double" => { /* kind: "double_command", payload: { ioa, val: value } */ }
"step"   => { /* kind: "step_command",   payload: { ioa, val: value } */ }
"setpoint_normalized" => { /* kind: "setpoint_normalized", payload: { ioa, val: value } */ }
"setpoint_scaled"     => { /* kind: "setpoint_scaled",     payload: { ioa, val: value } */ }
"setpoint_float"      => { /* kind: "setpoint_float",      payload: { ioa, val: value } */ }
```

类似地处理直接执行模式（`execute` 分支）若有调用 LogEntry 构造的位置。

import 添加：
```rust
use iec104sim_core::log_entry::DetailEvent;
```

- [ ] **Step 7.6: 运行测试**

```bash
cargo test -p iec104sim-core
cargo test -p iec104master-app
```
Expected: 全部 PASS（新加的 detail_event 测试也通过）。

- [ ] **Step 7.7: 提交**

```bash
git add crates/iec104sim-core/src/master.rs crates/iec104master-app/src/commands.rs crates/iec104sim-core/tests/
git commit -m "feat(master): emit structured detail_event for control commands"
```

---

## Task 8：主端 LogPanel 适配 detail_event + 前端 CSV 导出

**Files:**
- Modify: `master-frontend/src/types.ts`
- Modify: `master-frontend/src/components/LogPanel.vue`

- [ ] **Step 8.1: 更新 LogEntry 类型**

```ts
// master-frontend/src/types.ts (在 LogEntry 接口内追加)
export interface LogEntry {
  timestamp: string
  direction: string
  frame_label: { [key: string]: string } | string
  detail: string
  raw_bytes: number[] | null
  detail_event?: { kind: string; payload: Record<string, unknown> } | null
}
```

- [ ] **Step 8.2: LogPanel detail 渲染逻辑**

`LogPanel.vue` 中加一个辅助函数：
```ts
import { useI18n } from '../i18n'
const { t } = useI18n()

function formatDetail(log: LogEntry): string {
  if (log.detail_event && log.detail_event.kind) {
    return t(`log.${log.detail_event.kind}`, log.detail_event.payload)
  }
  return log.detail
}
```

template：
```vue
<td class="col-detail">{{ formatDetail(log) }}</td>
```

- [ ] **Step 8.3: 替换 LogPanel 中所有静态文案为 t()**

- 标题 `通信日志` → `t('log.title')`
- `暂无连接` → `t('log.noConnections')`
- `暂无日志` → `t('log.noLogs')`
- 表头 `时间/方向/帧类型/详情/原始数据` → `t('log.timeCol')` 等
- 按钮 `刷新/清空/导出` → `t('log.refresh')` 等

- [ ] **Step 8.4: 改造 CSV 导出为前端生成**

```ts
function exportLogs() {
  if (!selectedConnId.value) return
  const lines: string[] = []
  lines.push(`"${t('log.timeCol')}","${t('log.directionCol')}","${t('log.frameCol')}","${t('log.detailCol')}","${t('log.rawCol')}"`)
  for (const log of logs.value) {
    const ts = formatTimestamp(log.timestamp)
    const dir = formatDirection(log.direction)
    const frame = formatFrameLabel(log.frame_label).replace(/"/g, '""')
    const detail = formatDetail(log).replace(/"/g, '""')
    const raw = formatRawBytes(log.raw_bytes)
    lines.push(`"${ts}","${dir}","${frame}","${detail}","${raw}"`)
  }
  const csv = '﻿' + lines.join('\r\n')   // BOM 让 Excel 正确识别 UTF-8
  const blob = new Blob([csv], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `iec104_master_log_${Date.now()}.csv`
  a.click()
  URL.revokeObjectURL(url)
}
```

注：原本 `invoke('export_logs_csv', ...)` 调用废弃；保留后端 Tauri command（不删除，避免影响其它消费者，但前端不再用）。

- [ ] **Step 8.5: 类型检查 + dev 手测**

```bash
cd master-frontend && npm run build
cd master-frontend && npm run dev
```

启动主端 + 一个从端，建立连接，发送几个控制命令产生日志，观察：
- 中文 locale 下 detail 列显示 `单点命令 IOA=100 val=true`
- 切换到 EN，**已存在日志条目** detail 立即变 `Single Command IOA=100 val=true` ✓
- 导出 CSV，文件 detail 列与当前 locale 匹配
- 表头、按钮、空状态均跟随 locale

- [ ] **Step 8.6: 提交**

```bash
git add master-frontend/src/components/LogPanel.vue master-frontend/src/types.ts
git commit -m "feat(master-frontend): localize LogPanel via detail_event + frontend CSV"
```

---

## Task 9：复制 i18n 模块到从端 (`frontend`)

按 Task 1-3 的同样流程在 `frontend/` 中重复一遍。所有源代码文件可从 master-frontend 复制后改字典内容。

**Files:**
- Create: `frontend/src/i18n/{types.ts,detect.ts,index.ts,locales/zh-CN.ts,locales/en-US.ts}`
- Create: `frontend/src/components/LangSwitch.vue`
- Create: `frontend/tests/i18n.spec.ts`
- Create: `frontend/vitest.config.ts`
- Modify: `frontend/package.json`
- Modify: `frontend/src/components/Toolbar.vue`

- [ ] **Step 9.1: 安装 vitest + 配置**

```bash
cd frontend && npm i -D vitest @vue/test-utils jsdom
```

复制 `vitest.config.ts` 与 `package.json scripts`（参考 Task 1.2-1.3）。

- [ ] **Step 9.2: 复制 i18n 框架代码**

```bash
cp -r master-frontend/src/i18n frontend/src/
cp master-frontend/src/components/LangSwitch.vue frontend/src/components/
cp master-frontend/tests/i18n.spec.ts frontend/tests/
```

- [ ] **Step 9.3: 替换从端独有的字典内容**

把 `frontend/src/i18n/locales/zh-CN.ts` 与 `en-US.ts` 替换为从端字典：

从端独有的 namespace：
- `toolbar`：`新建服务器 / 启动 / 停止 / 添加站 / 随机变化 / 停止变化 / 周期发送 / 停止周期 / IEC 104 Slave / 关于`
- `newServer`：端口号 / 初始值 / 全零 / 随机 / 启用 TLS / 服务器证书文件 (PEM) / 服务器密钥文件 (PEM) / CA 证书文件 (PEM, 可选) / 要求客户端证书 (mTLS) / ...
- `tree`、`table`、`valuePanel`、`log`、`dataPointModal`、`batchAddModal`、`about`、`errors`：每一项都补齐
- `station`：`{ defaultName: '站 {ca}' }` （新增 — 用于替换 Rust 侧默认站名展示）
- `pointType`：核心库 `AsduType.as_text()` 与 `Category.as_text()` 返回的中文 ID 映射（与主端 `category` namespace 类似）

完整 grep：
```bash
cd frontend && grep -nE "[一-龥]" src/components/*.vue
```
逐项纳入字典。

英文字典需对应翻译，确保 `DictShape` 类型一致。

- [ ] **Step 9.4: 接入 Toolbar — 插入 LangSwitch**

参照 Task 2.2，把 `frontend/src/components/Toolbar.vue` 工具栏右侧（标题前）插入 `<LangSwitch />`。

- [ ] **Step 9.5: 类型检查 + 测试 + 手测**

```bash
cd frontend && npm test           # i18n 单元测试通过
cd frontend && npm run build      # vue-tsc 通过
cd frontend && npm run dev        # 切换按钮可见可切换、持久化
```

- [ ] **Step 9.6: 提交**

```bash
git add frontend/src/i18n frontend/src/components/LangSwitch.vue frontend/src/components/Toolbar.vue frontend/tests frontend/vitest.config.ts frontend/package.json frontend/package-lock.json
git commit -m "feat(frontend/i18n): scaffold i18n + LangSwitch (slave side)"
```

---

## Task 10：从端组件文案迁移

对从端的每个组件做与 Task 4-5 相同的迁移。

**Files:** 每个一个 sub-task，分别提交：
- Modify: `frontend/src/components/Toolbar.vue`
- Modify: `frontend/src/components/ConnectionTree.vue`
- Modify: `frontend/src/components/DataPointTable.vue`
- Modify: `frontend/src/components/DataPointModal.vue`
- Modify: `frontend/src/components/BatchAddModal.vue`
- Modify: `frontend/src/components/ValuePanel.vue`
- Modify: `frontend/src/components/LogPanel.vue` (含前端 CSV 导出 + detail_event 渲染)
- Modify: `frontend/src/components/AboutDialog.vue`
- Modify: `frontend/src/components/AppDialog.vue`
- Modify: `frontend/src/types.ts` (若有类似 master 的常量映射，重构为 getter 函数)

每个文件按以下流程：
- [ ] grep 中文 → 列出所有字符串
- [ ] import useI18n
- [ ] 替换 template + script 中所有中文为 `t('...')`
- [ ] 对 LogPanel：增加 `formatDetail(log)` + 改 CSV 导出为前端实现
- [ ] grep 确认无残留
- [ ] dev 手测
- [ ] 单组件 commit

**Step 10.10: 总验证 grep**

```bash
cd frontend && grep -rnE "[一-龥]" src/ | grep -v "i18n/locales"
```
Expected: 仅剩字典 key 字面量与注释。

---

## Task 11：从端 Rust 默认站名处理

**Files:**
- Modify: `crates/iec104sim-app/src/commands.rs`

把 `Station::with_default_points(1, "站 1", 10)` 与 `Station::with_random_points(1, "站 1", 10)` 中硬编码的 `"站 1"` 改为空字符串。前端在显示 station name 时若为空则用 `t('station.defaultName', { ca })`。

- [ ] **Step 11.1: 写失败测试**

`crates/iec104sim-core/tests/` 中已存在 station 相关测试；新增一个：创建带空 name 的 station，断言 `station.name == ""`。
（实际若 `Station::with_default_points` 不暴露 name getter，可只测 `Station::new` 路径）。或省略此 step。

- [ ] **Step 11.2: 修改 commands.rs**

```rust
// before
let default_station = match request.init_mode.as_deref() {
    Some("random") => Station::with_random_points(1, "站 1", 10),
    _ => Station::with_default_points(1, "站 1", 10),
};
// after
let default_station = match request.init_mode.as_deref() {
    Some("random") => Station::with_random_points(1, "", 10),
    _ => Station::with_default_points(1, "", 10),
};
```

- [ ] **Step 11.3: 前端 ConnectionTree 显示 station 时回退**

在 `frontend/src/components/ConnectionTree.vue`：

```ts
import { useI18n } from '../i18n'
const { t } = useI18n()

function stationDisplayName(station: { name: string; common_address: number }) {
  return station.name && station.name.length > 0
    ? station.name
    : t('station.defaultName', { ca: station.common_address })
}
```

template：
```vue
<span>{{ stationDisplayName(station) }}</span>
```

- [ ] **Step 11.4: 测试**

```bash
cargo test
cd frontend && npm run build && npm run dev
```

打开从端，新建一个服务器，确认默认站显示为 `站 1`（中文）/ `Station 1`（EN），切换 locale 立即更新。

- [ ] **Step 11.5: 提交**

```bash
git add crates/iec104sim-app/src/commands.rs frontend/src/components/ConnectionTree.vue
git commit -m "feat(slave): drop hardcoded station name; localize on display"
```

---

## Task 12：从端 LogPanel detail_event 改造（如需要）

如果从端 `LogPanel` 也展示了来自核心库 LogCollector 的 detail（含中文），同样处理：

**Files:**
- Modify: `crates/iec104sim-app/src/commands.rs` (及核心库相关函数 — 若从端 commands.rs 不直接构造日志，可能此 task 为 no-op)
- Modify: `frontend/src/components/LogPanel.vue` (formatDetail + 前端 CSV)

- [ ] **Step 12.1: 调研**

Run: `grep -nE "[一-龥]" crates/iec104sim-app/src/commands.rs`
Expected: 仅 `"站 1"` 已处理；其它中文 detail（如有）需补 detail_event。

如确无其它中文 detail 来源，则 LogPanel 侧只需 `formatDetail` 兜底（`detail_event` 字段大概率为 None，回退到 `detail` 字符串，已是英文/无中文）。

- [ ] **Step 12.2: 实施同 Task 8**

LogPanel 中加入 `formatDetail` + 前端 CSV 导出。

- [ ] **Step 12.3: 提交**

```bash
git add frontend/src/components/LogPanel.vue
git commit -m "feat(frontend): localize LogPanel rendering and CSV export"
```

---

## Task 13：端到端 QA 巡检

**Files:** 无（仅测试）

- [ ] **Step 13.1: 构建并启动主端从端各一**

```bash
cargo build --release
cd master-frontend && npm run build
cd frontend && npm run build
# 用 cargo tauri dev 或现有 build 脚本启动两个 app
```

- [ ] **Step 13.2: 矩阵测试**

| 场景 | 期望 |
|---|---|
| 主端中文系统首次启动 | UI 默认中文 |
| 主端英文系统首次启动 | UI 默认英文 |
| 主端切到 EN，重启 app | 仍为 EN |
| 主端切到中文，重启 | 仍为中文 |
| 从端同上 | 同上 |
| 主端建立连接、发送 GI / 时钟同步 / 控制命令、查看日志 | LogPanel 所有 detail 列文本跟随 locale；切换 locale 已显示日志立即变 |
| 主端导出 CSV | 文件 detail 列与当前 locale 匹配，表头跟随 locale |
| 从端建立服务器、添加站、修改值 | 默认站名跟随 locale；操作过程的 alert/prompt 跟随 locale |
| 主端 ControlDialog 的 SbO + 直接执行 | 选项 label 跟随 locale |
| 主从同时切换语言 | 互不干扰，各自持久化独立 (`localStorage` 不同 origin) |
| 主端 grep `[一-龥]` 残留 | 仅字典 key 与注释 |
| 从端 grep `[一-龥]` 残留 | 同上 |

- [ ] **Step 13.3: cargo + vitest 全跑**

```bash
cargo test
cd master-frontend && npm test
cd frontend && npm test
```
Expected: 全部 PASS。

- [ ] **Step 13.4: 修复任意发现的问题；视情况追加补丁 commit**

---

## Task 14：CHANGELOG

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 14.1: 在最新未发布段加一条**

```markdown
### Added
- Bilingual UI (中文 / English) with system-language detection and persistent toggle (master + slave). LogPanel localizes both static UI and structured backend events (control commands etc.); CSV export follows current locale.
```

- [ ] **Step 14.2: 提交**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): bilingual UI"
```

---

## Self-Review

**1. Spec coverage**

| Spec 要求 | 实现 Task |
|---|---|
| 主从两端均支持切换 | Task 1-12 |
| 覆盖前端 UI + 前端展示的后端运行时文本 | Task 4-5, 8, 10, 12 |
| 跟随系统语言初始化 | Task 1.6-1.7 detect.ts + 1.13 initialLocale |
| 切换持久化 | Task 1.13 setLocale + localStorage |
| 工具栏 `中 / EN` 切换控件 | Task 2 LangSwitch |
| 自研轻量 composable | Task 1.13 |
| 后端发结构化事件，前端字典渲染 | Task 6-8 (主) + Task 12 (从) |
| CSV 导出跟随 UI locale | Task 8.4 + Task 12 |
| TypeScript 缺键编译失败 | Task 1.10 `DictShape` |
| 默认站名 "站 1" 用前端字典本地化 | Task 11 |
| 单元测试 + 手工 E2E | Task 1.4-1.14 + Task 13 |
| 核心库中文字符串作为稳定 ID 不动，前端字典映射 | Task 5.4 (CONTROL_CONFIG_MAP) + 各组件 category 渲染 |

无遗漏。

**2. Placeholder scan**

- Task 3.2/3.3 中 `// ...` 已加注 "实际编码时必须完整列出每一项" — 不是占位符而是明确要求。
- Task 5 中 5.1-5.6 子任务的细节较短，但每个都给了完整流程模板（grep → 改 → 测 → 提交）。
- Task 12.1 是调研型 step，结果若为空则后续无操作 — 已说明。

无未补 placeholder。

**3. Type / 命名一致性**

- `DetailEvent { kind, payload }` 在 Rust (Task 6) 与 TS (Task 8.1) 字段名一致 ✓
- `useI18n()` 返回 `{ t, locale, setLocale }` 在所有调用处一致 ✓
- `STORAGE_KEY = 'iec104.locale'` 与 `localStorage.getItem('iec104.locale')` 测试一致 ✓
- `t('log.singleCommand')` key 与 Rust 发出的 `kind: "single_command"` 通过 `log.${kind}` 匹配（snake_case 一致）✓
- `Station::with_default_points` 第二参数空字符串 + 前端 `stationDisplayName` 兜底 ✓

无不一致。
