<script setup lang="ts">
import { ref, inject, watch, onMounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '../composables/useDialog'
import type { showAlert as ShowAlert } from '../composables/useDialog'
import type { ServerInfo, StationInfo } from '../types'
import { useI18n, localizeCategoryLabel } from '../i18n'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

const CATEGORIES = [
  '单点 (SP)',
  '双点 (DP)',
  '步位置 (ST)',
  '位串 (BO)',
  '归一化 (ME_NA)',
  '标度化 (ME_NB)',
  '浮点 (ME_NC)',
  '累计量 (IT)',
]

const sharedCategoryCounts = inject<Ref<Map<string, number>>>('categoryCounts')!

interface TreeServer {
  server: ServerInfo
  expanded: boolean
  stations: TreeStation[]
}

interface TreeStation {
  station: StationInfo
  expanded: boolean
  serverId: string
}

const emit = defineEmits<{
  (e: 'server-select', id: string, state: string): void
  (e: 'station-select', serverId: string, ca: number): void
  (e: 'category-select', serverId: string, ca: number, category: string): void
}>()

const treeRefreshKey = inject<Ref<number>>('treeRefreshKey')!
const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedCategory = inject<Ref<string | null>>('selectedCategory')!

const treeData = ref<TreeServer[]>([])
const contextMenu = ref({
  show: false,
  x: 0,
  y: 0,
  type: '' as 'server' | 'station',
  serverId: '',
  ca: 0,
  serverState: '',
})

async function loadTree() {
  try {
    const servers = await invoke<ServerInfo[]>('list_servers')
    const newTree: TreeServer[] = []

    for (const server of servers) {
      const existing = treeData.value.find(t => t.server.id === server.id)
      const stations = await invoke<StationInfo[]>('list_stations', { serverId: server.id })
      newTree.push({
        server,
        expanded: existing ? existing.expanded : true,
        stations: stations.map(s => ({
          station: s,
          expanded: existing?.stations.find(es => es.station.common_address === s.common_address)?.expanded ?? true,
          serverId: server.id,
        })),
      })
    }
    treeData.value = newTree
  } catch (e) {
    console.error('Failed to load tree:', e)
  }
}

watch(treeRefreshKey, () => loadTree())
onMounted(loadTree)

function toggleServer(ts: TreeServer) {
  ts.expanded = !ts.expanded
}

function toggleStation(tst: TreeStation) {
  tst.expanded = !tst.expanded
}

function selectServer(ts: TreeServer) {
  emit('server-select', ts.server.id, ts.server.state)
}

function selectStation(ts: TreeServer, tst: TreeStation) {
  emit('station-select', ts.server.id, tst.station.common_address)
}

function selectCategory(ts: TreeServer, tst: TreeStation, category: string) {
  emit('category-select', ts.server.id, tst.station.common_address, category)
}

function showContextMenuForServer(e: MouseEvent, ts: TreeServer) {
  e.preventDefault()
  contextMenu.value = {
    show: true,
    x: e.clientX,
    y: e.clientY,
    type: 'server',
    serverId: ts.server.id,
    ca: 0,
    serverState: ts.server.state,
  }
}

function showContextMenuForStation(e: MouseEvent, ts: TreeServer, tst: TreeStation) {
  e.preventDefault()
  contextMenu.value = {
    show: true,
    x: e.clientX,
    y: e.clientY,
    type: 'station',
    serverId: ts.server.id,
    ca: tst.station.common_address,
    serverState: '',
  }
}

function closeContextMenu() {
  contextMenu.value.show = false
}

async function ctxStartServer() {
  closeContextMenu()
  try {
    await invoke('start_server', { id: contextMenu.value.serverId })
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function ctxStopServer() {
  closeContextMenu()
  try {
    await invoke('stop_server', { id: contextMenu.value.serverId })
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function ctxDeleteServer() {
  closeContextMenu()
  try {
    await invoke('delete_server', { id: contextMenu.value.serverId })
    if (selectedServerId.value === contextMenu.value.serverId) {
      selectedServerId.value = null
    }
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function ctxDeleteStation() {
  closeContextMenu()
  try {
    await invoke('remove_station', {
      serverId: contextMenu.value.serverId,
      commonAddress: contextMenu.value.ca,
    })
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

function isServerSelected(ts: TreeServer): boolean {
  return ts.server.id === selectedServerId.value && selectedCA.value === null
}

function isStationSelected(ts: TreeServer, tst: TreeStation): boolean {
  return ts.server.id === selectedServerId.value
    && tst.station.common_address === selectedCA.value
    && selectedCategory.value === null
}

function isCategorySelected(ts: TreeServer, tst: TreeStation, category: string): boolean {
  return ts.server.id === selectedServerId.value
    && tst.station.common_address === selectedCA.value
    && selectedCategory.value === category
}
</script>

<template>
  <div class="connection-tree" @click="closeContextMenu">
    <div class="tree-header">{{ t('tree.title') }}</div>
    <div v-if="treeData.length === 0" class="tree-empty">{{ t('tree.noServers') }}</div>

    <div v-for="ts in treeData" :key="ts.server.id" class="tree-node-group">
      <!-- Server Node -->
      <div
        :class="['tree-node server-node', { selected: isServerSelected(ts) }]"
        @click.stop="selectServer(ts)"
        @contextmenu.prevent="showContextMenuForServer($event, ts)"
      >
        <span class="node-arrow" @click.stop="toggleServer(ts)">{{ ts.expanded ? '\u25BC' : '\u25B6' }}</span>
        <span :class="['node-status', ts.server.state === 'Running' ? 'running' : 'stopped']"></span>
        <span class="node-label">{{ ts.server.bind_address }}:{{ ts.server.port }}</span>
      </div>

      <!-- Station Nodes -->
      <template v-if="ts.expanded">
        <div v-for="tst in ts.stations" :key="tst.station.common_address" class="tree-child">
          <div
            :class="['tree-node station-node', { selected: isStationSelected(ts, tst) }]"
            @click.stop="selectStation(ts, tst)"
            @contextmenu.prevent="showContextMenuForStation($event, ts, tst)"
          >
            <span class="node-arrow" @click.stop="toggleStation(tst)">{{ tst.expanded ? '\u25BC' : '\u25B6' }}</span>
            <span class="node-label">{{ tst.station.name || `CA=${tst.station.common_address}` }}</span>
            <span class="node-badge">{{ tst.station.point_count }}</span>
          </div>

          <!-- Category Nodes -->
          <template v-if="tst.expanded">
            <div
              v-for="cat in CATEGORIES"
              :key="cat"
              :class="['tree-node category-node', { selected: isCategorySelected(ts, tst, cat) }]"
              @click.stop="selectCategory(ts, tst, cat)"
            >
              <span class="node-label">{{ localizeCategoryLabel(cat) }}</span>
              <span class="node-badge" v-if="sharedCategoryCounts.get(cat)">
                {{ sharedCategoryCounts.get(cat) }}
              </span>
            </div>
          </template>
        </div>
      </template>
    </div>

    <!-- Context Menu -->
    <div
      v-if="contextMenu.show"
      class="context-menu"
      :style="{ top: contextMenu.y + 'px', left: contextMenu.x + 'px' }"
      @click.stop
    >
      <template v-if="contextMenu.type === 'server'">
        <div
          v-if="contextMenu.serverState !== 'Running'"
          class="context-menu-item"
          @click="ctxStartServer"
        >{{ t('tree.ctxStartServer') }}</div>
        <div
          v-else
          class="context-menu-item"
          @click="ctxStopServer"
        >{{ t('tree.ctxStopServer') }}</div>
        <div class="context-menu-item danger" @click="ctxDeleteServer">{{ t('tree.ctxDeleteServer') }}</div>
      </template>
      <template v-if="contextMenu.type === 'station'">
        <div class="context-menu-item danger" @click="ctxDeleteStation">{{ t('tree.ctxDeleteStation') }}</div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.connection-tree {
  padding: 0;
  font-size: 13px;
  user-select: none;
  height: 100%;
  position: relative;
}

.tree-header {
  padding: 8px 12px;
  font-size: 11px;
  text-transform: uppercase;
  color: #6c7086;
  letter-spacing: 0.5px;
}

.tree-empty {
  padding: 16px 12px;
  color: #6c7086;
  font-size: 12px;
}

.tree-node {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px;
  cursor: pointer;
  border-radius: 3px;
  margin: 1px 4px;
}

.tree-node:hover {
  background: #313244;
}

.tree-node.selected {
  background: #89b4fa;
  color: #1e1e2e;
}

.tree-child {
  padding-left: 16px;
}

.category-node {
  padding-left: 32px;
}

.node-arrow {
  font-size: 8px;
  width: 12px;
  text-align: center;
  flex-shrink: 0;
  color: #6c7086;
}

.tree-node.selected .node-arrow {
  color: #1e1e2e;
}

.node-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.node-status.running {
  background: #a6e3a1;
}

.node-status.stopped {
  background: #585b70;
}

.node-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.node-badge {
  margin-left: auto;
  font-size: 10px;
  color: #6c7086;
  background: #313244;
  padding: 1px 6px;
  border-radius: 8px;
}

.tree-node.selected .node-badge {
  background: rgba(0, 0, 0, 0.2);
  color: #1e1e2e;
}

/* Context Menu */
.context-menu {
  position: fixed;
  background: #1e1e2e;
  border: 1px solid #45475a;
  border-radius: 6px;
  z-index: 999;
  min-width: 140px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.context-menu-item {
  padding: 8px 14px;
  font-size: 13px;
  color: #cdd6f4;
  cursor: pointer;
}

.context-menu-item:first-child {
  border-radius: 6px 6px 0 0;
}

.context-menu-item:last-child {
  border-radius: 0 0 6px 6px;
}

.context-menu-item:hover {
  background: #313244;
}

.context-menu-item.danger {
  color: #f38ba8;
}

.context-menu-item.danger:hover {
  background: #3d2a30;
}
</style>
