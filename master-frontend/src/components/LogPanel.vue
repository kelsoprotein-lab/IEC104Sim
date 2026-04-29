<script setup lang="ts">
import { ref, computed, inject, onMounted, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { LogEntry, ConnectionInfo } from '../types'
import { useI18n } from '../i18n'

const { t, locale } = useI18n()

interface Props {
  expanded: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'toggle'): void
}>()

const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!

const logs = ref<LogEntry[]>([])
// 倒序展示：最新条目在表格顶部，便于查看最近通讯
const displayLogs = computed(() => logs.value.slice().reverse())
const connectionList = ref<{ id: string; label: string }[]>([])
const selectedConnId = ref('')
let refreshTimer: number | null = null

async function loadConnections() {
  try {
    const conns = await invoke<ConnectionInfo[]>('list_connections')
    connectionList.value = conns.map(c => ({
      id: c.id,
      label: `${c.target_address}:${c.port}`,
    }))
    // Auto-select: prefer the currently selected connection in the tree
    if (selectedConnectionId.value && conns.some(c => c.id === selectedConnectionId.value)) {
      selectedConnId.value = selectedConnectionId.value
    } else if (connectionList.value.length > 0 && !selectedConnId.value) {
      selectedConnId.value = connectionList.value[0].id
    }
  } catch (_e) { /* ignore */ }
}

async function loadLogs() {
  if (!selectedConnId.value) return
  try {
    logs.value = await invoke<LogEntry[]>('get_communication_logs', {
      connectionId: selectedConnId.value,
    })
  } catch (_e) { /* ignore */ }
}

async function clearLogs() {
  if (!selectedConnId.value) return
  try {
    await invoke('clear_communication_logs', { connectionId: selectedConnId.value })
    logs.value = []
  } catch (_e) { /* ignore */ }
}

function formatDetail(log: LogEntry): string {
  if (log.detail_event && log.detail_event.kind) {
    return t(`log.${log.detail_event.kind}`, log.detail_event.payload)
  }
  return log.detail
}

function csvEscape(s: string): string {
  return s.replace(/"/g, '""')
}

function exportLogs() {
  if (!selectedConnId.value || logs.value.length === 0) return
  const lines: string[] = []
  lines.push([
    t('log.timeCol'), t('log.directionCol'), t('log.frameCol'), t('log.causeCol'), t('log.detailCol'), t('log.rawCol'),
  ].map(h => `"${csvEscape(h)}"`).join(','))
  for (const log of logs.value) {
    const ts = formatTimestamp(log.timestamp)
    const dir = formatDirection(log.direction)
    const frame = formatFrameLabel(log.frame_label)
    const cause = formatCause(log)
    const detail = formatDetail(log)
    const raw = formatRawBytes(log.raw_bytes)
    lines.push([
      `"${csvEscape(ts)}"`,
      `"${csvEscape(dir)}"`,
      `"${csvEscape(frame)}"`,
      `"${csvEscape(cause)}"`,
      `"${csvEscape(detail)}"`,
      `"${csvEscape(raw)}"`,
    ].join(','))
  }
  const csv = '﻿' + lines.join('\r\n')
  const blob = new Blob([csv], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `iec104_master_log_${Date.now()}.csv`
  a.click()
  URL.revokeObjectURL(url)
}

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts)
    if (isNaN(date.getTime())) return ts
    return date.toLocaleTimeString(locale.value, {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      fractionalSecondDigits: 3,
    } as Intl.DateTimeFormatOptions)
  } catch {
    return ts
  }
}

function formatDirection(dir: string): string {
  return dir.toUpperCase()
}

const FRAME_KEY_MAP: Record<string, string> = {
  i_frame: 'iFrame',
  s_frame: 'sFrame',
  u_start_act: 'uStartAct',
  u_start_con: 'uStartCon',
  u_stop_act: 'uStopAct',
  u_stop_con: 'uStopCon',
  u_test_act: 'uTestAct',
  u_test_con: 'uTestCon',
  general_interrogation: 'generalInterrogation',
  counter_read: 'counterRead',
  clock_sync: 'clockSync',
  single_command: 'singleCommand',
  double_command: 'doubleCommand',
  setpoint_normalized: 'setpointNormalized',
  setpoint_scaled: 'setpointScaled',
  setpoint_float: 'setpointFloat',
  connection_event: 'connectionEvent',
}

function formatFrameLabel(label: LogEntry['frame_label']): string {
  if (typeof label === 'string') {
    // Serde may serialize unit variants as a bare string (e.g. "s_frame")
    const dictKey = FRAME_KEY_MAP[label]
    return dictKey ? t(`log.frame.${dictKey}`, { value: '' }) : label
  }
  const keys = Object.keys(label)
  if (keys.length === 0) return ''
  const key = keys[0]
  const value = label[key]
  const dictKey = FRAME_KEY_MAP[key]
  return dictKey ? t(`log.frame.${dictKey}`, { value }) : key
}

function extractCot(log: LogEntry): number | null {
  const ev = log.detail_event
  if (ev) {
    const payload = ev.payload as Record<string, unknown> | undefined
    const v = payload?.cot
    if (typeof v === 'number') return v
  }
  const m = /COT=(\d+)/.exec(log.detail || '')
  return m ? parseInt(m[1], 10) : null
}

function formatCause(log: LogEntry): string {
  const cot = extractCot(log)
  if (cot === null) return ''
  const key = `log.cot.${cot}`
  const name = t(key)
  if (name === key) return t('log.cot.unknown', { cot })
  return `${cot} · ${name}`
}

function formatRawBytes(raw: number[] | null): string {
  if (!raw || raw.length === 0) return ''
  return raw.map(b => b.toString(16).toUpperCase().padStart(2, '0')).join(' ')
}

function dirClass(dir: string): string {
  return dir.toLowerCase()
}

function frameLabelClass(label: LogEntry['frame_label']): string {
  const key = typeof label === 'string' ? label : Object.keys(label)[0] || ''
  if (key === 'i_frame') return 'frame-i'
  if (key === 's_frame') return 'frame-s'
  if (key.startsWith('u_')) return 'frame-u'
  return ''
}

function startAutoRefresh() {
  if (refreshTimer) return
  refreshTimer = window.setInterval(() => {
    if (props.expanded) {
      loadConnections()
      loadLogs()
    }
  }, 2000)
}

function stopAutoRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
}

// When the selected connection in the tree changes, auto-select it in log panel
watch(selectedConnectionId, (newId) => {
  if (newId && connectionList.value.some(c => c.id === newId)) {
    selectedConnId.value = newId
  }
})

watch(() => props.expanded, (expanded) => {
  if (expanded) {
    loadConnections()
    loadLogs()
    startAutoRefresh()
  } else {
    stopAutoRefresh()
  }
})

watch(selectedConnId, () => loadLogs())

onMounted(async () => {
  await loadConnections()
  if (selectedConnId.value) await loadLogs()
  if (props.expanded) startAutoRefresh()
})
</script>

<template>
  <div :class="['log-panel', { expanded }]">
    <div class="log-header" @click="emit('toggle')">
      <span class="log-toggle">{{ expanded ? '\u25BC' : '\u25B2' }}</span>
      <span class="log-title">{{ t('log.title') }}</span>
      <span v-if="!expanded && logs.length > 0" class="log-count">{{ logs.length }}</span>
      <div class="log-controls" @click.stop>
        <select v-model="selectedConnId" class="conn-select" @change="loadLogs">
          <option v-for="conn in connectionList" :key="conn.id" :value="conn.id">{{ conn.label }}</option>
        </select>
        <button class="log-btn" @click="loadLogs">{{ t('log.refresh') }}</button>
        <button class="log-btn" @click="clearLogs">{{ t('log.clear') }}</button>
        <button class="log-btn" @click="exportLogs">{{ t('log.export') }}</button>
      </div>
    </div>

    <div v-if="expanded" class="log-body">
      <div v-if="connectionList.length === 0" class="log-empty">{{ t('log.noConnections') }}</div>
      <div v-else-if="logs.length === 0" class="log-empty">{{ t('log.noLogs') }}</div>
      <table v-else class="log-table">
        <thead>
          <tr>
            <th>{{ t('log.timeCol') }}</th>
            <th>{{ t('log.directionCol') }}</th>
            <th>{{ t('log.frameCol') }}</th>
            <th>{{ t('log.causeCol') }}</th>
            <th>{{ t('log.detailCol') }}</th>
            <th>{{ t('log.rawCol') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(log, idx) in displayLogs" :key="idx">
            <td class="col-time">{{ formatTimestamp(log.timestamp) }}</td>
            <td :class="['col-dir', dirClass(log.direction)]">{{ formatDirection(log.direction) }}</td>
            <td :class="['col-frame', frameLabelClass(log.frame_label)]">{{ formatFrameLabel(log.frame_label) }}</td>
            <td class="col-cause">{{ formatCause(log) }}</td>
            <td class="col-detail">{{ formatDetail(log) }}</td>
            <td class="col-raw">{{ formatRawBytes(log.raw_bytes) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.log-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.log-panel:not(.expanded) {
  height: 32px;
}

.log-header {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 8px;
  cursor: pointer;
  flex-shrink: 0;
  background: #1e1e2e;
}

.log-toggle {
  font-size: 10px;
  color: #6c7086;
  width: 16px;
  text-align: center;
}

.log-title {
  font-size: 12px;
  color: #6c7086;
}

.log-count {
  font-size: 10px;
  background: #89b4fa;
  color: #1e1e2e;
  padding: 0 6px;
  border-radius: 8px;
  font-weight: 600;
}

.log-controls {
  display: flex;
  gap: 4px;
  margin-left: auto;
}

.conn-select {
  padding: 2px 6px;
  background: #313244;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 11px;
  max-width: 160px;
}

.log-btn {
  padding: 2px 8px;
  background: transparent;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  cursor: pointer;
  font-size: 11px;
}

.log-btn:hover {
  background: #313244;
}

.log-body {
  flex: 1;
  overflow-y: auto;
  background: #11111b;
}

.log-empty {
  padding: 24px;
  text-align: center;
  color: #6c7086;
  font-size: 12px;
}

.log-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

.log-table th,
.log-table td {
  padding: 4px 10px;
  text-align: left;
  border-bottom: 1px solid #1e1e2e;
}

.log-table th {
  background: #181825;
  color: #6c7086;
  font-weight: 500;
  position: sticky;
  top: 0;
}

.col-time {
  font-family: 'SF Mono', 'Fira Code', monospace;
  color: #6c7086;
  width: 100px;
}

.col-dir {
  font-weight: 600;
  width: 40px;
}

.col-dir.rx { color: #89b4fa; }
.col-dir.tx { color: #a6e3a1; }

.col-frame {
  font-family: 'SF Mono', 'Fira Code', monospace;
  width: 130px;
  white-space: nowrap;
}

.col-frame.frame-u { color: #cba6f7; }
.col-frame.frame-i { color: #89dceb; }
.col-frame.frame-s { color: #f9e2af; }

.col-cause {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 11px;
  color: #f9e2af;
  width: 160px;
  white-space: nowrap;
}

.col-detail {
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.col-raw {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 11px;
  color: #585b70;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
