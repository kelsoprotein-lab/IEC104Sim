<script setup lang="ts">
import { ref, inject, watch, onMounted, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { LogEntry } from '../types'
import { useI18n } from '../i18n'

const { t } = useI18n()

interface Props {
  expanded: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'toggle'): void
}>()

const selectedServerId = inject<Ref<string | null>>('selectedServerId')!

const logs = ref<LogEntry[]>([])
const isLoading = ref(false)
const error = ref<string | null>(null)
let refreshTimer: number | null = null

async function loadLogs() {
  if (!selectedServerId.value) {
    logs.value = []
    return
  }
  isLoading.value = true
  try {
    logs.value = await invoke<LogEntry[]>('get_communication_logs', {
      serverId: selectedServerId.value,
    })
  } catch (e) {
    error.value = String(e)
  }
  isLoading.value = false
}

async function clearLogs() {
  if (!selectedServerId.value) return
  try {
    await invoke('clear_communication_logs', {
      serverId: selectedServerId.value,
    })
    logs.value = []
  } catch (e) {
    error.value = String(e)
  }
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
  if (!selectedServerId.value || logs.value.length === 0) return
  const lines: string[] = []
  lines.push([
    t('log.timeCol'), t('log.directionCol'), t('log.frameCol'), t('log.detailCol'),
  ].map(h => `"${csvEscape(h)}"`).join(','))
  for (const log of logs.value) {
    lines.push([
      `"${csvEscape(formatTimestamp(log.timestamp))}"`,
      `"${csvEscape(log.direction)}"`,
      `"${csvEscape(formatFrameLabel(log.frame_label))}"`,
      `"${csvEscape(formatDetail(log))}"`,
    ].join(','))
  }
  const csv = '﻿' + lines.join('\r\n')
  const blob = new Blob([csv], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `iec104_log_${Date.now()}.csv`
  a.click()
  URL.revokeObjectURL(url)
}

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts)
    return date.toLocaleTimeString()
  } catch {
    return ts
  }
}

function formatFrameLabel(label: { [key: string]: string } | string): string {
  if (typeof label === 'string') return label
  // label is an object like { "I": "..." } or { "S": "" } or { "U": "STARTDT_ACT" }
  const entries = Object.entries(label)
  if (entries.length === 0) return '-'
  const [key, value] = entries[0]
  return value ? `${key}: ${value}` : key
}

function toggleExpanded() {
  emit('toggle')
}

function startAutoRefresh() {
  if (refreshTimer) return
  refreshTimer = window.setInterval(() => {
    if (props.expanded && selectedServerId.value) {
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

watch(() => props.expanded, async (expanded) => {
  if (expanded) {
    if (selectedServerId.value) await loadLogs()
    startAutoRefresh()
  } else {
    stopAutoRefresh()
  }
})

watch(selectedServerId, async () => {
  if (props.expanded && selectedServerId.value) {
    await loadLogs()
  } else {
    logs.value = []
  }
})

onMounted(async () => {
  if (props.expanded && selectedServerId.value) {
    await loadLogs()
    startAutoRefresh()
  }
})

onUnmounted(() => stopAutoRefresh())
</script>

<template>
  <div :class="['log-panel', { expanded }]">
    <div class="log-header" @click="toggleExpanded">
      <span class="log-toggle">{{ expanded ? '\u25BC' : '\u25B2' }}</span>
      <span class="log-title">{{ t('log.title') }}</span>
      <div class="log-controls" @click.stop>
        <button class="log-btn" @click="loadLogs" :title="t('log.titleRefresh')">{{ t('log.refresh') }}</button>
        <button class="log-btn" @click="clearLogs" :title="t('log.titleClear')">{{ t('log.clear') }}</button>
        <button class="log-btn" @click="exportLogs" :title="t('log.titleExport')">{{ t('log.export') }}</button>
      </div>
    </div>

    <div v-if="expanded" class="log-body">
      <div v-if="isLoading" class="log-loading">{{ t('log.loading') }}</div>
      <div v-else-if="!selectedServerId" class="log-empty">{{ t('log.chooseServer') }}</div>
      <div v-else-if="logs.length === 0" class="log-empty">{{ t('log.noLogs') }}</div>
      <table v-else class="log-table">
        <thead>
          <tr>
            <th>{{ t('log.timeCol') }}</th>
            <th>{{ t('log.directionCol') }}</th>
            <th>{{ t('log.frameCol') }}</th>
            <th>{{ t('log.detailCol') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(log, idx) in logs" :key="idx">
            <td class="col-time">{{ formatTimestamp(log.timestamp) }}</td>
            <td :class="['col-dir', log.direction.toLowerCase()]">{{ log.direction }}</td>
            <td class="col-frame">{{ formatFrameLabel(log.frame_label) }}</td>
            <td class="col-detail">{{ formatDetail(log) }}</td>
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
  transition: height 0.2s ease;
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

.log-controls {
  display: flex;
  gap: 4px;
  margin-left: auto;
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

.log-loading,
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
  font-family: 'SF Mono', 'Fira Code', monospace;
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
  color: #6c7086;
  width: 80px;
}

.col-dir {
  font-weight: 600;
  width: 40px;
}

.col-dir.rx {
  color: #a6e3a1;
}

.col-dir.tx {
  color: #89b4fa;
}

.col-frame {
  width: 120px;
  color: #cdd6f4;
}

.col-detail {
  color: #a6adc8;
}
</style>
