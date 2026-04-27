<script setup lang="ts">
import { ref, inject, watch, onMounted, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ConnectionInfo } from '../types'
import { useI18n } from '../i18n'

const { t } = useI18n()

const emit = defineEmits<{
  (e: 'connection-select', id: string, state: string): void
  (e: 'category-select', connectionId: string, category: string): void
}>()

const treeRefreshKey = inject<Ref<number>>('treeRefreshKey')!
const refreshTree = inject<() => void>('refreshTree')!
const changedCategories = inject<Ref<Map<string, Set<string>>>>('changedCategories')!
const sharedCategoryCounts = inject<Ref<Map<string, Map<string, number>>>>('categoryCounts')!

// Local flash state: keyed by "${connId}|${categoryLabel}" to keep flashes per-connection.
const flashingCategories = ref<Set<string>>(new Set())
const flashTimers = new Map<string, number>()
const flashKey = (connId: string, cat: string) => `${connId}|${cat}`

watch(changedCategories, (map) => {
  if (map.size === 0) return
  for (const [connId, cats] of map) {
    for (const cat of cats) {
      const key = flashKey(connId, cat)
      flashingCategories.value.add(key)
      const prev = flashTimers.get(key)
      if (prev) clearTimeout(prev)
      flashTimers.set(key, window.setTimeout(() => {
        flashingCategories.value.delete(key)
        flashTimers.delete(key)
      }, 3000))
    }
  }
  // Clear after consuming
  changedCategories.value = new Map()
})

onUnmounted(() => {
  for (const t of flashTimers.values()) clearTimeout(t)
})

// IEC 104 data categories matching the backend DataCategory enum display names
const DATA_CATEGORIES = [
  { key: 'single_point', label: '单点 (SP)' },
  { key: 'double_point', label: '双点 (DP)' },
  { key: 'step_position', label: '步位置 (ST)' },
  { key: 'bitstring', label: '位串 (BO)' },
  { key: 'normalized_measured', label: '归一化 (ME_NA)' },
  { key: 'scaled_measured', label: '标度化 (ME_NB)' },
  { key: 'float_measured', label: '浮点 (ME_NC)' },
  { key: 'integrated_totals', label: '累计量 (IT)' },
]

interface TreeConnection {
  info: ConnectionInfo
  expanded: boolean
}

const connections = ref<TreeConnection[]>([])
const selectedNodeId = ref<string | null>(null)

// Context menu
const contextMenu = ref<{ visible: boolean; x: number; y: number; connId: string }>({
  visible: false, x: 0, y: 0, connId: ''
})

// Per-connection count lookup — read from the scoped shared ref in the template.
function countFor(connId: string, label: string): number {
  return sharedCategoryCounts.value.get(connId)?.get(label) ?? 0
}

async function loadTree() {
  try {
    const conns = await invoke<ConnectionInfo[]>('list_connections')
    const activeIds = new Set(conns.map(c => c.id))
    const newTree: TreeConnection[] = []
    for (const conn of conns) {
      const existing = connections.value.find(c => c.info.id === conn.id)
      newTree.push({
        info: conn,
        expanded: existing?.expanded ?? true,
      })
    }
    connections.value = newTree

    // GC stale entries for connections that no longer exist
    const staleCounts = [...sharedCategoryCounts.value.keys()].filter(k => !activeIds.has(k))
    if (staleCounts.length > 0) {
      const next = new Map(sharedCategoryCounts.value)
      for (const id of staleCounts) next.delete(id)
      sharedCategoryCounts.value = next
    }
    const staleFlash = [...changedCategories.value.keys()].filter(k => !activeIds.has(k))
    if (staleFlash.length > 0) {
      const next = new Map(changedCategories.value)
      for (const id of staleFlash) next.delete(id)
      changedCategories.value = next
    }
  } catch (_e) {
    // Ignore errors on load
  }
}

watch(treeRefreshKey, loadTree)
onMounted(loadTree)

function selectConnection(conn: TreeConnection) {
  selectedNodeId.value = conn.info.id
  emit('connection-select', conn.info.id, conn.info.state)
}

function selectCategory(conn: TreeConnection, cat: { key: string; label: string }) {
  selectedNodeId.value = `${conn.info.id}:${cat.key}`
  // Only emit category-select, not connection-select (avoids category being reset to null)
  emit('category-select', conn.info.id, cat.label)
}

function toggleExpand(conn: TreeConnection) {
  conn.expanded = !conn.expanded
}

function showContextMenu(e: MouseEvent, connId: string) {
  e.preventDefault()
  contextMenu.value = { visible: true, x: e.clientX, y: e.clientY, connId }
}

function hideContextMenu() {
  contextMenu.value.visible = false
}

async function ctxDeleteConnection() {
  try {
    await invoke('delete_connection', { id: contextMenu.value.connId })
    refreshTree()
  } catch (_e) { /* ignore */ }
  hideContextMenu()
}

function stateClass(state: string): string {
  const s = state.toLowerCase()
  if (s === 'connected') return 'connected'
  if (s.includes('error')) return 'error'
  return 'disconnected'
}
</script>

<template>
  <div class="tree-container" @click="hideContextMenu">
    <div class="tree-header">{{ t('tree.title') }}</div>

    <div v-if="connections.length === 0" class="tree-empty">
      {{ t('tree.noConnections') }}
    </div>

    <div v-for="conn in connections" :key="conn.info.id" class="tree-node-group">
      <!-- Connection node -->
      <div
        :class="['tree-node', { selected: selectedNodeId === conn.info.id }]"
        @click="selectConnection(conn)"
        @contextmenu="showContextMenu($event, conn.info.id)"
      >
        <span class="node-expand" @click.stop="toggleExpand(conn)">
          {{ conn.expanded ? '\u25BC' : '\u25B6' }}
        </span>
        <span :class="['node-status', stateClass(conn.info.state)]"></span>
        <span class="node-label">{{ conn.info.target_address }}:{{ conn.info.port }}</span>
        <span class="node-ca">CA:{{ conn.info.common_address }}</span>
      </div>

      <!-- Category children -->
      <div v-if="conn.expanded" class="tree-children">
        <div
          v-for="cat in DATA_CATEGORIES"
          :key="cat.key"
          :class="['tree-node', 'tree-child', {
            selected: selectedNodeId === `${conn.info.id}:${cat.key}`,
            'cat-flash': flashingCategories.has(flashKey(conn.info.id, cat.label)),
          }]"
          @click="selectCategory(conn, cat)"
        >
          <span class="node-label">{{ t(`category.${cat.key}`) }}</span>
          <span class="node-count" v-if="countFor(conn.info.id, cat.label) > 0">
            {{ countFor(conn.info.id, cat.label) }}
          </span>
        </div>
      </div>
    </div>

    <!-- Context Menu -->
    <div v-if="contextMenu.visible" class="context-menu" :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }">
      <div class="ctx-item danger" @click="ctxDeleteConnection">{{ t('tree.deleteConnection') }}</div>
    </div>
  </div>
</template>

<style scoped>
.tree-container {
  padding: 4px 0;
  font-size: 12px;
  user-select: none;
}

.tree-header {
  padding: 8px 12px;
  font-size: 11px;
  text-transform: uppercase;
  color: #6c7086;
  letter-spacing: 0.5px;
}

.tree-empty {
  padding: 24px 12px;
  color: #6c7086;
  text-align: center;
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
  background: #1e1e2e;
}

.tree-node.selected {
  background: #89b4fa;
  color: #1e1e2e;
}

.tree-node.selected .node-ca,
.tree-node.selected .node-count {
  color: #1e1e2e;
  opacity: 0.7;
}

.tree-child {
  padding-left: 28px;
}

.node-expand {
  font-size: 8px;
  width: 12px;
  text-align: center;
  color: #6c7086;
}

.node-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.node-status.connected { background: #a6e3a1; }
.node-status.disconnected { background: #6c7086; }
.node-status.error { background: #f38ba8; }

.node-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.node-ca {
  font-size: 10px;
  color: #6c7086;
}

.node-count {
  font-size: 10px;
  color: #6c7086;
  background: #313244;
  padding: 0 5px;
  border-radius: 8px;
  min-width: 18px;
  text-align: center;
}

/* Context Menu */
.context-menu {
  position: fixed;
  background: #1e1e2e;
  border: 1px solid #45475a;
  border-radius: 6px;
  padding: 4px 0;
  z-index: 999;
  min-width: 120px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.ctx-item {
  padding: 6px 14px;
  cursor: pointer;
  font-size: 12px;
}

.ctx-item:hover {
  background: #313244;
}

.ctx-item.danger {
  color: #f38ba8;
}

.cat-flash {
  background: rgba(250, 179, 135, 0.2) !important;
}

.cat-flash .node-label {
  color: #fab387;
  font-weight: 600;
}
</style>
