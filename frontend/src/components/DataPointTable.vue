<script setup lang="ts">
import { ref, inject, watch, computed, nextTick, onMounted, onUnmounted, shallowRef, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '../composables/useDialog'
import type { showAlert as ShowAlert } from '../composables/useDialog'
import type { DataPointInfo } from '../types'
import DataPointModal from './DataPointModal.vue'
import BatchAddModal from './BatchAddModal.vue'

const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

const emit = defineEmits<{
  (e: 'point-select', points: { ioa: number; value: string }[]): void
}>()

const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedCategory = inject<Ref<string | null>>('selectedCategory')!
const dataRefreshKey = inject<Ref<number>>('dataRefreshKey')!

// === Core data: plain JS Map + shallowRef (same pattern as master DataTable) ===
let dataMap = new Map<number, DataPointInfo>()
const displayPoints = shallowRef<DataPointInfo[]>([])
const categoryCounts = inject<Ref<Map<string, number>>>('categoryCounts')!
let currentServerId: string | null = null
let currentCA: number | null = null

// === UI state ===
const selectedRows = ref<DataPointInfo[]>([])
const lastClickedIndex = ref(-1)
const editingCell = ref<{ ioa: number } | null>(null)
const editValue = ref('')
const isLoading = ref(false)
const searchQuery = ref('')
const scrollContainer = ref<HTMLDivElement | null>(null)
const showAddModal = ref(false)
const showBatchModal = ref(false)
const changedIoas = ref<Set<number>>(new Set())
const changeTimers = new Map<number, number>()

// === Virtual scroll (same pattern as master DataTable) ===
const ROW_HEIGHT = 28
const OVERSCAN = 10
const scrollTop = ref(0)
const containerHeight = ref(400)

// === Rebuild display array from dataMap + update category counts ===
function updateDisplay() {
  const arr = Array.from(dataMap.values())
  arr.sort((a, b) => a.ioa - b.ioa)
  displayPoints.value = arr
  // Compute realtime category counts — backend returns Chinese category names directly
  const counts = new Map<string, number>()
  for (const p of arr) {
    counts.set(p.category, (counts.get(p.category) || 0) + 1)
  }
  categoryCounts.value = counts
}

function markChanged(ioa: number) {
  changedIoas.value.add(ioa)
  const prev = changeTimers.get(ioa)
  if (prev) clearTimeout(prev)
  changeTimers.set(ioa, window.setTimeout(() => {
    changedIoas.value.delete(ioa)
    changeTimers.delete(ioa)
  }, 3000))
}

// === Load data points: merge into existing map, never replace ===
async function loadDataPoints() {
  if (!selectedServerId.value || selectedCA.value === null) return
  try {
    const points = await invoke<DataPointInfo[]>('list_data_points', {
      serverId: selectedServerId.value,
      commonAddress: selectedCA.value,
    })
    for (const p of points) {
      const old = dataMap.get(p.ioa)
      if (!old || old.value !== p.value) {
        markChanged(p.ioa)
      }
      dataMap.set(p.ioa, p)
    }
    updateDisplay()
  } catch (e) {
    console.error('Failed to load data points:', e)
  }
}

// === Watchers ===
watch([selectedServerId, selectedCA], async ([, ], [, ]) => {
  const srvId = selectedServerId.value
  const ca = selectedCA.value
  if (!srvId || ca === null) {
    // Cleared selection
    dataMap = new Map()
    displayPoints.value = []
    currentServerId = null
    currentCA = null
    changedIoas.value.clear()
    for (const t of changeTimers.values()) clearTimeout(t)
    changeTimers.clear()
    selectedRows.value = []
    emitSelection()
    return
  }
  // Only reset if server or CA actually changed
  if (srvId !== currentServerId || ca !== currentCA) {
    dataMap = new Map()
    displayPoints.value = []
    currentServerId = srvId
    currentCA = ca
    changedIoas.value.clear()
    for (const t of changeTimers.values()) clearTimeout(t)
    changeTimers.clear()
    selectedRows.value = []
    emitSelection()
  }
  await loadDataPoints()
})

watch(dataRefreshKey, () => {
  if (currentServerId && currentCA !== null) {
    loadDataPoints()
  }
})

// === Auto-polling: refresh data points every 2s to pick up control command changes ===
let pollTimer: ReturnType<typeof setInterval> | null = null

function startPolling() {
  stopPolling()
  pollTimer = setInterval(() => {
    if (currentServerId && currentCA !== null) {
      loadDataPoints()
    }
  }, 2000)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

onMounted(() => { startPolling() })

onUnmounted(() => {
  stopPolling()
  for (const t of changeTimers.values()) clearTimeout(t)
})

// === Filtered points ===
const filteredPoints = computed(() => {
  let pts = displayPoints.value
  if (selectedCategory.value) {
    pts = pts.filter(p => p.category === selectedCategory.value)
  }
  const q = searchQuery.value.trim()
  if (!q) return pts
  if (/^\d+$/.test(q)) {
    const num = Number(q)
    return pts.filter(p => p.ioa === num || p.ioa.toString().includes(q))
  }
  const lower = q.toLowerCase()
  return pts.filter(p =>
    p.name.toLowerCase().includes(lower)
    || p.asdu_type.toLowerCase().includes(lower)
  )
})

// Virtual scroll state
const totalHeight = computed(() => filteredPoints.value.length * ROW_HEIGHT)
const visibleStart = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - OVERSCAN))
const visibleEnd = computed(() => {
  const count = Math.ceil(containerHeight.value / ROW_HEIGHT) + OVERSCAN * 2
  return Math.min(filteredPoints.value.length, visibleStart.value + count)
})
const visibleRows = computed(() => filteredPoints.value.slice(visibleStart.value, visibleEnd.value))
const offsetY = computed(() => visibleStart.value * ROW_HEIGHT)

function onScroll(e: Event) {
  const el = e.target as HTMLElement
  scrollTop.value = el.scrollTop
  containerHeight.value = el.clientHeight
}

function isSelected(point: DataPointInfo): boolean {
  return selectedRows.value.some(r => r.ioa === point.ioa)
}

function selectRow(e: MouseEvent, point: DataPointInfo) {
  const list = filteredPoints.value
  const idx = list.indexOf(point)
  const isCtrl = e.ctrlKey || e.metaKey

  if (e.shiftKey && lastClickedIndex.value >= 0) {
    const start = Math.min(lastClickedIndex.value, idx)
    const end = Math.max(lastClickedIndex.value, idx)
    selectedRows.value = list.slice(start, end + 1)
  } else if (isCtrl) {
    if (isSelected(point)) {
      selectedRows.value = selectedRows.value.filter(r => r.ioa !== point.ioa)
    } else {
      selectedRows.value = [...selectedRows.value, point]
    }
    lastClickedIndex.value = idx
  } else {
    selectedRows.value = [point]
    lastClickedIndex.value = idx
  }

  emitSelection()
}

function emitSelection() {
  const points = selectedRows.value.map(r => ({
    ioa: r.ioa,
    value: r.value,
  }))
  emit('point-select', points)
}

function handleTableKeydown(e: KeyboardEvent) {
  if (editingCell.value) return
  const list = filteredPoints.value
  if (list.length === 0) return

  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    let currentIdx = -1
    if (selectedRows.value.length > 0) {
      const last = selectedRows.value[selectedRows.value.length - 1]
      currentIdx = list.findIndex(r => r.ioa === last.ioa)
    }

    let nextIdx: number
    if (e.key === 'ArrowDown') {
      nextIdx = currentIdx < list.length - 1 ? currentIdx + 1 : currentIdx
    } else {
      nextIdx = currentIdx > 0 ? currentIdx - 1 : 0
    }

    if (nextIdx >= 0 && nextIdx < list.length) {
      selectedRows.value = [list[nextIdx]]
      lastClickedIndex.value = nextIdx
      emitSelection()

      nextTick(() => {
        const container = scrollContainer.value
        if (!container) return
        const rows = container.querySelectorAll('tbody tr')
        if (rows[nextIdx]) {
          rows[nextIdx].scrollIntoView({ block: 'nearest' })
        }
      })
    }
  }
}

function startEdit(point: DataPointInfo) {
  editingCell.value = { ioa: point.ioa }
  editValue.value = point.value
}

async function commitEdit() {
  if (!editingCell.value || !selectedServerId.value || currentCA === null) return
  const { ioa } = editingCell.value
  const value = editValue.value
  editingCell.value = null

  try {
    await invoke('update_data_point', {
      serverId: selectedServerId.value,
      commonAddress: currentCA,
      ioa,
      value,
    })
    await loadDataPoints()
  } catch (e) {
    await showAlert(String(e))
  }
}

function cancelEdit() {
  editingCell.value = null
}

function handleEditKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    commitEdit()
  } else if (e.key === 'Escape') {
    cancelEdit()
  }
}

function formatTimestamp(ts: string | null): string {
  if (!ts) return '-'
  return ts
}

function onPointAdded() {
  dataRefreshKey.value++
}

// Context menu for delete
const contextMenu = ref({ show: false, x: 0, y: 0, ioa: 0 })

function showContextMenu(e: MouseEvent, point: DataPointInfo) {
  e.preventDefault()
  contextMenu.value = { show: true, x: e.clientX, y: e.clientY, ioa: point.ioa }
}

function closeContextMenu() {
  contextMenu.value.show = false
}

async function deletePoint() {
  const ioa = contextMenu.value.ioa
  contextMenu.value.show = false
  if (!selectedServerId.value || currentCA === null) return
  try {
    await invoke('remove_data_point', {
      serverId: selectedServerId.value,
      commonAddress: currentCA,
      ioa,
    })
    if (selectedRows.value.some(r => r.ioa === ioa)) {
      selectedRows.value = selectedRows.value.filter(r => r.ioa !== ioa)
      emitSelection()
    }
    await loadDataPoints()
  } catch (e) {
    await showAlert(String(e))
  }
}

// Allow parent to directly trigger data load (bypasses async watch timing issues)
defineExpose({ loadData: loadDataPoints })
</script>

<template>
  <div class="data-point-table" @click="closeContextMenu">
    <div class="table-header-bar">
      <span class="table-title">
        {{ selectedCategory || '全部数据点' }}
      </span>
      <input
        v-model="searchQuery"
        class="search-input"
        type="text"
        placeholder="搜索 IOA / 名称..."
      />
      <button
        class="add-btn"
        :disabled="!selectedServerId || currentCA === null"
        @click="showAddModal = true"
        title="添加数据点"
      >+</button>
      <button
        class="add-btn batch"
        :disabled="!selectedServerId || currentCA === null"
        @click="showBatchModal = true"
        title="批量添加"
      >批量</button>
      <span class="table-count">{{ filteredPoints.length }} 个数据点</span>
    </div>

    <div v-if="isLoading" class="table-loading">加载中...</div>
    <div v-else-if="!selectedServerId || currentCA === null" class="table-empty">
      请在左侧树形导航中选择一个站
    </div>
    <div v-else-if="filteredPoints.length === 0" class="table-empty">
      暂无数据点
    </div>

    <div
      v-else
      ref="scrollContainer"
      class="table-scroll-container"
      tabindex="0"
      @scroll="onScroll"
      @keydown="handleTableKeydown"
    >
      <!-- Fixed header -->
      <table class="table">
        <thead>
          <tr>
            <th class="th-ioa">IOA</th>
            <th class="th-type">ASDU 类型</th>
            <th class="th-name">名称</th>
            <th class="th-value">值</th>
            <th class="th-quality">品质</th>
            <th class="th-timestamp">时间戳</th>
          </tr>
        </thead>
      </table>
      <!-- Virtual scroll body -->
      <div v-if="filteredPoints.length > 0" :style="{ height: totalHeight + 'px', position: 'relative' }">
        <table class="table table-body" :style="{ transform: `translateY(${offsetY}px)` }">
          <tbody>
            <tr
              v-for="point in visibleRows"
              :key="point.ioa"
              :class="{
                selected: isSelected(point),
                'value-changed': changedIoas.has(point.ioa)
              }"
              @click="selectRow($event, point)"
              @contextmenu.prevent="showContextMenu($event, point)"
            >
              <td class="col-ioa">{{ point.ioa }}</td>
              <td class="col-type">{{ point.asdu_type }}</td>
              <td class="col-name">{{ point.name || '-' }}</td>
              <td :class="['col-value', { 'value-highlight': changedIoas.has(point.ioa) }]" @dblclick.stop="startEdit(point)">
                <template v-if="editingCell?.ioa === point.ioa">
                  <input
                    v-model="editValue"
                    class="edit-input"
                    type="text"
                    autofocus
                    @blur="commitEdit"
                    @keydown="handleEditKeydown"
                    @click.stop
                  />
                </template>
                <template v-else>
                  <span class="value-text">{{ point.value }}</span>
                </template>
              </td>
              <td class="col-quality">
                <span v-if="point.quality_iv" class="quality-dot invalid" title="Invalid">IV</span>
                <span v-else class="quality-dot ok" title="Good"></span>
              </td>
              <td class="col-timestamp">{{ formatTimestamp(point.timestamp) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- Context Menu -->
    <div
      v-if="contextMenu.show"
      class="context-menu"
      :style="{ top: contextMenu.y + 'px', left: contextMenu.x + 'px' }"
      @click.stop
    >
      <div class="context-menu-item danger" @click="deletePoint">删除数据点</div>
    </div>

    <!-- Add Data Point Modal -->
    <DataPointModal
      :visible="showAddModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      @close="showAddModal = false"
      @added="onPointAdded"
    />

    <!-- Batch Add Modal -->
    <BatchAddModal
      :visible="showBatchModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      @close="showBatchModal = false"
      @added="onPointAdded"
    />
  </div>
</template>

<style scoped>
.data-point-table {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.table-header-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid #313244;
  flex-shrink: 0;
}

.table-title {
  font-size: 12px;
  font-weight: 600;
  color: #cdd6f4;
  white-space: nowrap;
}

.search-input {
  flex: 1;
  min-width: 0;
  padding: 4px 8px;
  background: #313244;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 12px;
  outline: none;
}

.search-input:focus {
  border-color: #89b4fa;
}

.search-input::placeholder {
  color: #6c7086;
}

.add-btn {
  padding: 2px 8px;
  background: #313244;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #a6e3a1;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  line-height: 1;
}

.add-btn.batch {
  font-size: 11px;
  font-weight: 400;
}

.add-btn:hover:not(:disabled) {
  background: #45475a;
}

.add-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.table-count {
  font-size: 11px;
  color: #6c7086;
  white-space: nowrap;
}

.table-loading,
.table-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #6c7086;
  font-size: 13px;
}

.table-scroll-container {
  flex: 1;
  overflow-y: auto;
  outline: none;
}

.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

.table thead {
  position: sticky;
  top: 0;
  z-index: 1;
}

.table th {
  background: #1e1e2e;
  color: #6c7086;
  font-weight: 500;
  text-align: left;
  padding: 6px 10px;
  border-bottom: 1px solid #313244;
  position: sticky;
  top: 0;
}

.table td {
  padding: 5px 10px;
  border-bottom: 1px solid #1e1e2e;
  cursor: pointer;
}

.table tbody tr:hover {
  background: #1e1e2e;
}

.table tbody tr.selected {
  background: #89b4fa;
  color: #1e1e2e;
}

.table tbody tr.value-changed {
  background: rgba(250, 179, 135, 0.15);
}

.col-ioa {
  font-family: 'SF Mono', 'Fira Code', monospace;
  width: 70px;
  color: #89b4fa;
}

.table tbody tr.selected .col-ioa {
  color: #1e1e2e;
}

.col-type {
  width: 100px;
}

.col-name {
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-value {
  width: 120px;
  font-family: 'SF Mono', 'Fira Code', monospace;
  transition: color 0.3s;
}

.value-text {
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.col-value.value-highlight {
  color: #fab387;
  font-weight: 700;
}

.col-quality {
  width: 40px;
  text-align: center;
}

.quality-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.quality-dot.ok {
  background: #a6e3a1;
}

.quality-dot.invalid {
  background: #f38ba8;
  width: auto;
  height: auto;
  border-radius: 3px;
  padding: 1px 4px;
  font-size: 10px;
  font-weight: 600;
  color: #1e1e2e;
}

.col-timestamp {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 11px;
  color: #6c7086;
  width: 100px;
}

.table tbody tr.selected .col-timestamp {
  color: #45475a;
}

.edit-input {
  width: 90px;
  padding: 2px 6px;
  background: #1e1e2e;
  border: 1px solid #89b4fa;
  border-radius: 3px;
  color: #cdd6f4;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 12px;
  outline: none;
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
  border-radius: 6px;
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
