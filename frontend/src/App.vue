<script setup lang="ts">
import { ref, provide, onMounted } from 'vue'
import Toolbar from './components/Toolbar.vue'
import ConnectionTree from './components/ConnectionTree.vue'
import DataPointTable from './components/DataPointTable.vue'
import ValuePanel from './components/ValuePanel.vue'
import LogPanel from './components/LogPanel.vue'
import AppDialog from './components/AppDialog.vue'
import UpdateDialog from './components/UpdateDialog.vue'
import { invoke } from '@tauri-apps/api/core'
import { showAlert, showConfirm, showPrompt, dialogKey } from './composables/useDialog'

const dataPointTableRef = ref<InstanceType<typeof DataPointTable> | null>(null)

// Shared state
const selectedServerId = ref<string | null>(null)
const selectedServerState = ref<string>('Stopped')
const selectedCA = ref<number | null>(null)
const selectedCategory = ref<string | null>(null)
const selectedPoints = ref<{ ioa: number; value: string }[]>([])
const logExpanded = ref(false)

// Provide shared state to children
provide('selectedServerId', selectedServerId)
provide('selectedServerState', selectedServerState)
provide('selectedCA', selectedCA)
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

// Realtime category counts derived from DataPointTable's dataMap
const categoryCounts = ref<Map<string, number>>(new Map())
provide('categoryCounts', categoryCounts)
provide(dialogKey, { showAlert, showConfirm, showPrompt })

function handleServerSelect(id: string, state: string) {
  selectedServerId.value = id
  selectedServerState.value = state
  selectedCA.value = null
  selectedCategory.value = null
  selectedPoints.value = []
}

function handleStationSelect(serverId: string, ca: number) {
  selectedServerId.value = serverId
  selectedCA.value = ca
  selectedCategory.value = null
  selectedPoints.value = []
  dataPointTableRef.value?.loadData()
}

function handleCategorySelect(serverId: string, ca: number, category: string) {
  selectedServerId.value = serverId
  selectedCA.value = ca
  selectedCategory.value = category
  selectedPoints.value = []
  dataPointTableRef.value?.loadData()
}

function handlePointSelect(points: { ioa: number; value: string }[]) {
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

onMounted(() => {
  setTimeout(checkUpdate, 2000)
})

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
        @server-select="handleServerSelect"
        @station-select="handleStationSelect"
        @category-select="handleCategorySelect"
      />
    </aside>
    <main class="content-area">
      <DataPointTable
        ref="dataPointTableRef"
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
  grid-template-columns: 240px 1fr 280px;
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
