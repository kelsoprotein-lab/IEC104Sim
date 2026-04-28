<script setup lang="ts">
import { ref, provide, onMounted, onUnmounted } from 'vue'
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
const selectedCategory = ref<string | null>(null)
const selectedPoints = ref<ReceivedDataPointInfo[]>([])
const logExpanded = ref(false)

// Provide shared state to children
provide('selectedConnectionId', selectedConnectionId)
provide('selectedConnectionState', selectedConnectionState)
provide('selectedCategory', selectedCategory)
provide('selectedPoints', selectedPoints)

// Tree refresh trigger
const treeRefreshKey = ref(0)
provide('treeRefreshKey', treeRefreshKey)

function refreshTree() {
  treeRefreshKey.value++
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

// Per-connection category counts: connId -> Map<categoryLabel, count>
const categoryCounts = ref<Map<string, Map<string, number>>>(new Map())
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
  setTimeout(checkUpdate, 2000)
})

onUnmounted(() => {
  unlistenConnState?.()
})

function handleConnectionSelect(id: string, state: string) {
  const changed = selectedConnectionId.value !== id
  selectedConnectionId.value = id
  selectedConnectionState.value = state
  // Only clear category when switching to a different connection
  if (changed) {
    selectedCategory.value = null
    selectedPoints.value = []
  }
}

function handleCategorySelect(connectionId: string, category: string) {
  selectedConnectionId.value = connectionId
  selectedConnectionState.value = selectedConnectionState.value // preserve
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

async function checkUpdate() {
  try {
    const meta = await invoke<{ version: string; notes: string; pub_date?: string | null } | null>('check_for_update')
    if (meta) {
      updateMeta.value = meta
      updateVisible.value = true
    }
  } catch (e) {
    console.warn('update check failed', e)
  }
}

function snoozeUpdate() {
  if (updateMeta.value) {
    invoke('snooze_update', { version: updateMeta.value.version }).catch(() => {})
  }
}
</script>

<template>
  <div :class="['app-layout', { 'log-expanded': logExpanded }]">
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

.app-layout {
  display: grid;
  grid-template-columns: 260px 1fr 280px;
  grid-template-rows: 42px 1fr 32px;
  grid-template-areas:
    "toolbar toolbar toolbar"
    "tree content panel"
    "log log log";
  height: 100vh;
  width: 100vw;
}

.app-layout.log-expanded {
  grid-template-rows: 42px 1fr 200px;
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

.log-area {
  grid-area: log;
  background: #1e1e2e;
  border-top: 1px solid #313244;
  overflow: hidden;
}
</style>
