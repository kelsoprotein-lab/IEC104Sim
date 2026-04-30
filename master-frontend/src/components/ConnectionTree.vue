<script setup lang="ts">
import { ref, inject, watch, onMounted, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ConnectionInfo, ChangedCategoriesMap, CategoryCountsMap } from '../types'
import { useI18n } from '../i18n'

const { t } = useI18n()

const emit = defineEmits<{
  (e: 'connection-select', id: string, state: string): void
  // ca === null means "all CAs combined" (matches the connection-level
  // category click); otherwise it's the specific CA the user picked.
  (e: 'category-select', connectionId: string, category: string, ca: number | null): void
}>()

const treeRefreshKey = inject<Ref<number>>('treeRefreshKey')!
const refreshTree = inject<() => void>('refreshTree')!
// Provided by Toolbar: opens the new-connection dialog in edit mode for the
// given connection id. Optional — if Toolbar isn't mounted (shouldn't happen
// in this app), the menu item just no-ops.
const openEditConnection = inject<((connId: string) => void) | null>('openEditConnection', null)
const changedCategories = inject<Ref<ChangedCategoriesMap>>('changedCategories')!
const sharedCategoryCounts = inject<Ref<CategoryCountsMap>>('categoryCounts')!

// Flash key 用真实 (connId, ca, category) 三元组。single-CA 视图也能拿到唯一
// CA (`conn.info.common_addresses[0]`),所以不需要 wildcard sentinel。
const flashingCategories = ref<Set<string>>(new Set())
const flashTimers = new Map<string, number>()
const flashKey = (connId: string, ca: number, cat: string) => `${connId}|${ca}|${cat}`

watch(changedCategories, (map) => {
  if (map.size === 0) return
  for (const [connId, byCa] of map) {
    for (const [ca, cats] of byCa) {
      for (const cat of cats) {
        const key = flashKey(connId, ca, cat)
        flashingCategories.value.add(key)
        const prev = flashTimers.get(key)
        if (prev) clearTimeout(prev)
        flashTimers.set(key, window.setTimeout(() => {
          flashingCategories.value.delete(key)
          flashTimers.delete(key)
        }, 3000))
      }
    }
  }
  changedCategories.value = new Map()
})

onUnmounted(() => {
  for (const t of flashTimers.values()) clearTimeout(t)
})

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
  // Per-CA expanded state. Keyed by CA value.
  caExpanded: Record<number, boolean>
}

const connections = ref<TreeConnection[]>([])
// Selected node id is one of:
//   "<connId>"                     — the connection node itself
//   "<connId>:ca:<ca>"             — a specific CA group node
//   "<connId>:ca:<ca>:<catKey>"    — a category under a specific CA
//   "<connId>:<catKey>"            — a category under "all CAs" (single-CA case)
const selectedNodeId = ref<string | null>(null)

const contextMenu = ref<{ visible: boolean; x: number; y: number; connId: string }>({
  visible: false, x: 0, y: 0, connId: ''
})

// Look up a count for a specific (conn, ca, category) bucket. ca=null sums
// across every CA (used when the connection has only one CA configured and
// the tree is rendered flat).
function countFor(connId: string, label: string, ca: number | null): number {
  const byCa = sharedCategoryCounts.value.get(connId)
  if (!byCa) return 0
  if (ca !== null) {
    return byCa.get(ca)?.get(label) ?? 0
  }
  let total = 0
  for (const m of byCa.values()) total += m.get(label) ?? 0
  return total
}

// Should we render per-CA sub-nodes for this connection? Yes if the user
// configured multiple CAs (`common_addresses.length > 1`); otherwise the
// classic flat tree is friendlier.
function isMultiCA(conn: TreeConnection): boolean {
  return conn.info.common_addresses.length > 1
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
        caExpanded: existing?.caExpanded ?? Object.fromEntries(
          conn.common_addresses.map((ca) => [ca, true])
        ),
      })
    }
    connections.value = newTree

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

function selectCategory(conn: TreeConnection, cat: { key: string; label: string }, ca: number | null) {
  const id = ca === null
    ? `${conn.info.id}:${cat.key}`
    : `${conn.info.id}:ca:${ca}:${cat.key}`
  selectedNodeId.value = id
  emit('category-select', conn.info.id, cat.label, ca)
}

function toggleExpand(conn: TreeConnection) {
  conn.expanded = !conn.expanded
}

function toggleCAExpand(conn: TreeConnection, ca: number) {
  conn.caExpanded[ca] = !conn.caExpanded[ca]
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

function ctxEditConnection() {
  const id = contextMenu.value.connId
  hideContextMenu()
  openEditConnection?.(id)
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
          {{ conn.expanded ? '▼' : '▶' }}
        </span>
        <span :class="['node-status', stateClass(conn.info.state)]"></span>
        <span class="node-label">{{ conn.info.target_address }}:{{ conn.info.port }}</span>
        <span class="node-ca">CA:{{ conn.info.common_addresses.join(',') }}</span>
      </div>

      <!-- Children -->
      <div v-if="conn.expanded" class="tree-children">

        <!-- Multi-CA: connection -> CA -> category -->
        <template v-if="isMultiCA(conn)">
          <div v-for="ca in conn.info.common_addresses" :key="ca" class="ca-group">
            <div
              :class="['tree-node', 'ca-node', { selected: selectedNodeId === `${conn.info.id}:ca:${ca}` }]"
              @click="toggleCAExpand(conn, ca)"
            >
              <span class="node-expand">{{ conn.caExpanded[ca] ? '▼' : '▶' }}</span>
              <span class="ca-badge">CA {{ ca }}</span>
            </div>
            <div v-if="conn.caExpanded[ca]" class="ca-children">
              <div
                v-for="cat in DATA_CATEGORIES"
                :key="`${ca}-${cat.key}`"
                :class="['tree-node', 'tree-child', 'tree-grand', {
                  selected: selectedNodeId === `${conn.info.id}:ca:${ca}:${cat.key}`,
                  'cat-flash': flashingCategories.has(flashKey(conn.info.id, ca, cat.label)),
                }]"
                @click="selectCategory(conn, cat, ca)"
              >
                <span class="node-label">{{ t(`category.${cat.key}`) }}</span>
                <span class="node-count" v-if="countFor(conn.info.id, cat.label, ca) > 0">
                  {{ countFor(conn.info.id, cat.label, ca) }}
                </span>
              </div>
            </div>
          </div>
        </template>

        <!-- Single-CA: classic flat tree (counts summed across all CAs, which
             in this case is just the one configured CA). -->
        <template v-else>
          <div
            v-for="cat in DATA_CATEGORIES"
            :key="cat.key"
            :class="['tree-node', 'tree-child', {
              selected: selectedNodeId === `${conn.info.id}:${cat.key}`,
              'cat-flash': flashingCategories.has(flashKey(conn.info.id, conn.info.common_addresses[0], cat.label)),
            }]"
            @click="selectCategory(conn, cat, null)"
          >
            <span class="node-label">{{ t(`category.${cat.key}`) }}</span>
            <span class="node-count" v-if="countFor(conn.info.id, cat.label, null) > 0">
              {{ countFor(conn.info.id, cat.label, null) }}
            </span>
          </div>
        </template>
      </div>
    </div>

    <div v-if="contextMenu.visible" class="context-menu" :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }">
      <div class="ctx-item" @click="ctxEditConnection">{{ t('tree.editConnection') }}</div>
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
.tree-node.selected .node-count,
.tree-node.selected .ca-badge {
  color: #1e1e2e;
  opacity: 0.85;
}

.tree-child {
  padding-left: 28px;
}

.tree-grand {
  padding-left: 48px;
}

.ca-node {
  padding-left: 18px;
  font-weight: 500;
}

.ca-badge {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 10px;
  background: #313244;
  color: #cba6f7;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.3px;
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
