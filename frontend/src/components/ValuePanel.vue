<script setup lang="ts">
import { inject, computed, ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '../composables/useDialog'
import type { showAlert as ShowAlert } from '../composables/useDialog'
import type { DataPointInfo } from '../types'
import { useI18n, localizeCategoryLabel } from '../i18n'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!
const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedPoints = inject<Ref<{ ioa: number; asdu_type: string; value: string }[]>>('selectedPoints')!

const hasSelection = computed(() => selectedPoints.value.length > 0)
const isSingle = computed(() => selectedPoints.value.length === 1)
const firstPoint = computed(() => selectedPoints.value[0] ?? null)

// Detailed info for the selected point
const pointDetail = ref<DataPointInfo | null>(null)

watch(
  () => [selectedServerId.value, selectedCA.value, selectedPoints.value] as const,
  async ([serverId, ca, points]) => {
    if (!serverId || ca === null || points.length !== 1) {
      pointDetail.value = null
      return
    }
    try {
      const allPoints = await invoke<DataPointInfo[]>('list_data_points', {
        serverId,
        commonAddress: ca,
      })
      pointDetail.value = allPoints.find(
        p => p.ioa === points[0].ioa && p.asdu_type === points[0].asdu_type,
      ) ?? null
    } catch {
      pointDetail.value = null
    }
  },
  { immediate: true },
)

// Editing state
const isEditing = ref(false)
const editValue = ref('')

function startEdit() {
  if (!firstPoint.value) return
  editValue.value = firstPoint.value.value
  isEditing.value = true
}

function cancelEdit() {
  isEditing.value = false
}

watch(selectedPoints, () => {
  isEditing.value = false
})

async function writeValue() {
  if (!selectedServerId.value || selectedCA.value === null || !firstPoint.value) return
  isEditing.value = false
  try {
    await invoke('update_data_point', {
      serverId: selectedServerId.value,
      commonAddress: selectedCA.value,
      ioa: firstPoint.value.ioa,
      asduType: pointDetail.value?.asdu_type ?? '',
      value: editValue.value,
    })
    // 不立即 refreshData：list_data_points 在大数据点场景下耗时数百 ms，
    // 立即触发会卡 UI；2s polling 自然跟上即可。本面板的 pointDetail
    // 也通过下一次 polling 重读。
    if (pointDetail.value) {
      pointDetail.value = { ...pointDetail.value, value: editValue.value }
    }
  } catch (e) {
    await showAlert(String(e))
  }
}

function handleEditKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    writeValue()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    cancelEdit()
  }
}
</script>

<template>
  <div class="value-panel">
    <div class="panel-header">{{ t('valuePanel.title') }}</div>

    <div v-if="!hasSelection" class="empty-state">
      {{ t('valuePanel.selectPointHint') }}
    </div>

    <template v-else-if="isSingle && pointDetail">
      <!-- Single point detail -->
      <div class="detail-section">
        <div class="section-title">{{ t('valuePanel.sectionInfo') }}</div>
        <div class="detail-row">
          <span class="detail-label">IOA</span>
          <span class="detail-value mono">{{ pointDetail.ioa }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.asduType') }}</span>
          <span class="detail-value">{{ pointDetail.asdu_type }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.category') }}</span>
          <span class="detail-value">{{ localizeCategoryLabel(pointDetail.category) }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.name') }}</span>
          <span class="detail-value">{{ pointDetail.name || '-' }}</span>
        </div>
        <div v-if="pointDetail.comment" class="detail-row">
          <span class="detail-label">{{ t('valuePanel.comment') }}</span>
          <span class="detail-value">{{ pointDetail.comment }}</span>
        </div>
      </div>

      <div class="detail-section">
        <div class="section-title">{{ t('valuePanel.sectionCurrent') }}</div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.value') }}</span>
          <span class="detail-value mono editable" @click="startEdit">{{ pointDetail.value }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.quality') }}</span>
          <span class="detail-value">
            <span v-if="pointDetail.quality_iv" class="quality-badge invalid">{{ t('valuePanel.qualityInvalid') }}</span>
            <span v-else class="quality-badge ok">{{ t('valuePanel.qualityValid') }}</span>
          </span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{{ t('valuePanel.timestamp') }}</span>
          <span class="detail-value mono">{{ pointDetail.timestamp || '-' }}</span>
        </div>
      </div>

      <div class="detail-section">
        <div class="section-title">{{ t('valuePanel.sectionWrite') }}</div>
        <div class="write-row">
          <input
            v-model="editValue"
            class="write-input"
            type="text"
            :placeholder="t('valuePanel.valuePlaceholder')"
            @keydown="handleEditKeydown"
          />
          <button class="write-btn" @click="writeValue">{{ t('valuePanel.write') }}</button>
        </div>
      </div>
    </template>

    <template v-else>
      <!-- Multiple selection -->
      <div class="multi-info">
        <div class="detail-section">
          <div class="section-title">{{ t('valuePanel.sectionMultiSelect') }}</div>
          <div class="detail-row">
            <span class="detail-label">{{ t('valuePanel.countLabel') }}</span>
            <span class="detail-value">{{ selectedPoints.length }} {{ t('table.countSuffix') }}</span>
          </div>
          <div class="ioa-list">
            <span v-for="p in selectedPoints" :key="`${p.ioa}-${p.asdu_type}`" class="ioa-chip">
              {{ p.ioa }}
            </span>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.value-panel {
  padding: 0;
  font-size: 13px;
}

.panel-header {
  padding: 8px 12px;
  font-size: 11px;
  text-transform: uppercase;
  color: #6c7086;
  letter-spacing: 0.5px;
}

.empty-state {
  padding: 24px 12px;
  color: #6c7086;
  text-align: center;
  font-size: 12px;
}

.detail-section {
  padding: 8px 0;
  border-bottom: 1px solid #313244;
}

.section-title {
  padding: 4px 16px;
  font-size: 11px;
  color: #89b4fa;
  text-transform: uppercase;
  font-weight: 600;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 4px 16px;
}

.detail-label {
  color: #6c7086;
  font-size: 12px;
  flex-shrink: 0;
}

.detail-value {
  color: #cdd6f4;
  font-size: 12px;
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-value.mono {
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.detail-value.editable {
  cursor: pointer;
  border-radius: 3px;
  padding: 0 4px;
  user-select: none;
}

.detail-value.editable:hover {
  background: #313244;
}

.quality-badge {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 3px;
  font-size: 11px;
  font-weight: 600;
}

.quality-badge.ok {
  background: #a6e3a1;
  color: #1e1e2e;
}

.quality-badge.invalid {
  background: #f38ba8;
  color: #1e1e2e;
}

.write-row {
  display: flex;
  gap: 6px;
  padding: 6px 16px;
}

.write-input {
  flex: 1;
  padding: 6px 10px;
  background: #11111b;
  border: 1px solid #45475a;
  border-radius: 6px;
  color: #cdd6f4;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 13px;
  outline: none;
  box-sizing: border-box;
}

.write-input:focus {
  border-color: #89b4fa;
}

.write-btn {
  padding: 6px 14px;
  background: #89b4fa;
  color: #1e1e2e;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
}

.write-btn:hover {
  background: #74c7ec;
}

.ioa-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 6px 16px;
}

.ioa-chip {
  padding: 2px 8px;
  background: #313244;
  border-radius: 4px;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 11px;
  color: #89b4fa;
}
</style>
