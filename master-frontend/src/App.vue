<script setup lang="ts">
import { ref, shallowRef, computed, provide, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import Toolbar from './components/Toolbar.vue'
import ConnectionTree from './components/ConnectionTree.vue'
import DataTable from './components/DataTable.vue'
import ValuePanel from './components/ValuePanel.vue'
import LogPanel from './components/LogPanel.vue'
import AppDialog from './components/AppDialog.vue'
import UpdateDialog from './components/UpdateDialog.vue'
import { showAlert, showConfirm, showPrompt, dialogKey } from './composables/useDialog'
import type { ReceivedDataPointInfo } from './types'

// Shared state
const selectedConnectionId = ref<string | null>(null)
const selectedConnectionState = ref<string>('Disconnected')
// Multi-CA: which Common Address inside the selected connection is the user
// looking at? `null` means "all CAs combined" (legacy single-CA behaviour).
const selectedCA = ref<number | null>(null)
const selectedCategory = ref<string | null>(null)
// shallowRef: 选中可达 15k+ 行（Ctrl+A）；deep ref 会在切换连接清空时卡几百 ms。
const selectedPoints = shallowRef<ReceivedDataPointInfo[]>([])
const logExpanded = ref(false)

const LOG_H_KEY = 'iec104.logPanel.height'
function readSavedHeight(): number {
  try {
    const v = parseInt(localStorage.getItem(LOG_H_KEY) || '', 10)
    if (!isNaN(v) && v > 0) return v
  } catch { /* ignore */ }
  return 220
}
const logHeight = ref<number>(readSavedHeight())

function clampLogHeight(h: number): number {
  const max = Math.max(120, Math.floor(window.innerHeight * 0.7))
  return Math.min(max, Math.max(80, h))
}

const gridRows = computed(() => {
  if (!logExpanded.value) return '42px 1fr 0 32px'
  return `42px 1fr 4px ${logHeight.value}px`
})

function startResize(e: MouseEvent) {
  e.preventDefault()
  const startY = e.clientY
  const startH = logHeight.value
  document.body.style.cursor = 'ns-resize'
  document.body.style.userSelect = 'none'
  function onMove(ev: MouseEvent) {
    logHeight.value = clampLogHeight(startH + (startY - ev.clientY))
  }
  function onUp() {
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
    try { localStorage.setItem(LOG_H_KEY, String(logHeight.value)) } catch { /* ignore */ }
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

// Provide shared state to children
provide('selectedConnectionId', selectedConnectionId)
provide('selectedConnectionState', selectedConnectionState)
provide('selectedCA', selectedCA)
provide('selectedCategory', selectedCategory)
provide('selectedPoints', selectedPoints)

// Tree refresh trigger
const treeRefreshKey = ref(0)
provide('treeRefreshKey', treeRefreshKey)

// 80ms 防抖：连续 connection-state 事件（disconnect→delete→reconnect）合并为一次重载。
let refreshTreePending: number | null = null
function refreshTree() {
  if (refreshTreePending !== null) return
  refreshTreePending = window.setTimeout(() => {
    refreshTreePending = null
    treeRefreshKey.value++
  }, 80)
}
provide('refreshTree', refreshTree)

// Data refresh trigger
const dataRefreshKey = ref(0)
provide('dataRefreshKey', dataRefreshKey)

function refreshData() {
  dataRefreshKey.value++
}
provide('refreshData', refreshData)

// Per-connection tree flash effect: connId -> set of changed category labels
const changedCategories = ref<Map<string, Set<string>>>(new Map())
provide('changedCategories', changedCategories)

// Per-connection-per-CA category counts: connId -> Map<CA, Map<categoryLabel, count>>
// (DataTable updates this from the points stream; ConnectionTree reads it.)
const categoryCounts = ref<Map<string, Map<number, Map<string, number>>>>(new Map())
provide('categoryCounts', categoryCounts)

provide(dialogKey, { showAlert, showConfirm, showPrompt })

// Listen for backend connection state events
let unlistenConnState: (() => void) | null = null

onMounted(async () => {
  unlistenConnState = await listen<{ id: string; state: string }>('connection-state', (event) => {
    const { id, state } = event.payload
    if (selectedConnectionId.value === id) {
      selectedConnectionState.value = state
    }
    refreshTree()
  })
  setTimeout(() => checkUpdate(false), 2000)
})

onUnmounted(() => {
  unlistenConnState?.()
  if (refreshTreePending !== null) {
    clearTimeout(refreshTreePending)
    refreshTreePending = null
  }
})

function handleConnectionSelect(id: string, state: string) {
  const changed = selectedConnectionId.value !== id
  selectedConnectionId.value = id
  selectedConnectionState.value = state
  // Only clear category when switching to a different connection
  if (changed) {
    selectedCA.value = null
    selectedCategory.value = null
    selectedPoints.value = []
  }
}

function handleCategorySelect(connectionId: string, category: string, ca: number | null) {
  selectedConnectionId.value = connectionId
  selectedConnectionState.value = selectedConnectionState.value // preserve
  selectedCA.value = ca
  selectedCategory.value = category
}

function handlePointSelect(points: ReceivedDataPointInfo[]) {
  selectedPoints.value = points
}

function toggleLog() {
  logExpanded.value = !logExpanded.value
}

const updateMeta = ref<{ version: string; notes: string; pub_date?: string | null } | null>(null)
const updateVisible = ref(false)

async function checkUpdate(force = false): Promise<{ version: string; notes: string; pub_date?: string | null } | null> {
  try {
    const meta = await invoke<{ version: string; notes: string; pub_date?: string | null } | null>('check_for_update', { force })
    if (meta) {
      updateMeta.value = meta
      updateVisible.value = true
    }
    return meta
  } catch (e) {
    console.warn('update check failed', e)
    return null
  }
}
provide('checkUpdate', checkUpdate)

function snoozeUpdate() {
  if (updateMeta.value) {
    invoke('snooze_update', { version: updateMeta.value.version }).catch(() => {})
  }
}
</script>

<template>
  <div :class="['app-layout', { 'log-expanded': logExpanded }]" :style="{ gridTemplateRows: gridRows }">
    <header class="toolbar-area">
      <Toolbar />
    </header>

    <aside class="tree-area">
      <ConnectionTree
        @connection-select="handleConnectionSelect"
        @category-select="handleCategorySelect"
      />
    </aside>
    <main class="content-area">
      <DataTable
        @point-select="handlePointSelect"
      />
    </main>
    <aside class="panel-area">
      <ValuePanel />
    </aside>

    <div
      v-show="logExpanded"
      class="log-resizer"
      role="separator"
      aria-orientation="horizontal"
      @mousedown="startResize"
    />
    <footer class="log-area">
      <LogPanel :expanded="logExpanded" @toggle="toggleLog" />
    </footer>
    <AppDialog />
    <UpdateDialog
      :visible="updateVisible"
      :version="updateMeta?.version ?? ''"
      :notes="updateMeta?.notes ?? ''"
      @close="updateVisible = false"
      @snooze="snoozeUpdate"
    />
  </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  height: 100%;
  width: 100%;
  overflow: hidden;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
  background: #11111b;
  color: #cdd6f4;
}

/* Dark scrollbars across the app — overrides macOS "Always show" white tracks */
*::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}
*::-webkit-scrollbar-track {
  background: #181825;
}
*::-webkit-scrollbar-thumb {
  background: #313244;
  border-radius: 5px;
  border: 2px solid #181825;
}
*::-webkit-scrollbar-thumb:hover {
  background: #45475a;
}
*::-webkit-scrollbar-corner {
  background: #181825;
}
* {
  scrollbar-color: #313244 #181825;
  scrollbar-width: thin;
}

.app-layout {
  display: grid;
  grid-template-columns: 260px 1fr 280px;
  grid-template-rows: 42px 1fr 0 32px;
  grid-template-areas:
    "toolbar toolbar toolbar"
    "tree content panel"
    "resizer resizer resizer"
    "log log log";
  height: 100vh;
  width: 100vw;
}

.toolbar-area {
  grid-area: toolbar;
  background: #1e1e2e;
  border-bottom: 1px solid #313244;
}

.tree-area {
  grid-area: tree;
  background: #181825;
  border-right: 1px solid #313244;
  overflow-y: auto;
}

.content-area {
  grid-area: content;
  background: #11111b;
  overflow: hidden;
}

.panel-area {
  grid-area: panel;
  background: #181825;
  border-left: 1px solid #313244;
  overflow-y: auto;
}

.log-resizer {
  grid-area: resizer;
  height: 4px;
  background: #313244;
  cursor: ns-resize;
  transition: background 0.15s;
  user-select: none;
}

.log-resizer:hover {
  background: #89b4fa;
}

.log-area {
  grid-area: log;
  background: #1e1e2e;
  border-top: 1px solid #313244;
  overflow: hidden;
}
</style>
