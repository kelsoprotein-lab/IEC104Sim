<script setup lang="ts">
import { ref, computed, watch, inject } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '../composables/useDialog'
import type { showAlert as ShowAlert } from '../composables/useDialog'
import { useI18n } from '../i18n'
import { ASDU_TYPE_OPTIONS } from '../constants/asduTypes'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

interface Props {
  visible: boolean
  serverId: string
  commonAddress: number
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  added: []
}>()

const ASDU_TYPES = computed(() =>
  ASDU_TYPE_OPTIONS.map(o => ({ value: o.value, label: t(o.labelKey) }))
)

const startIoa = ref(0)
const count = ref(10)
const formAsduType = ref('MSpNa1')
const namePrefix = ref('')
const isSaving = ref(false)

const endIoa = computed(() => startIoa.value + count.value - 1)

const isValid = computed(() => {
  return count.value > 0 && count.value <= 100000 && startIoa.value >= 0
})

watch(() => props.visible, (visible) => {
  if (visible) {
    startIoa.value = 0
    count.value = 10
    formAsduType.value = 'MSpNa1'
    namePrefix.value = ''
    isSaving.value = false
  }
})

async function handleConfirm() {
  if (!isValid.value) return
  isSaving.value = true

  try {
    await invoke('batch_add_data_points', {
      request: {
        server_id: props.serverId,
        common_address: props.commonAddress,
        start_ioa: startIoa.value,
        count: count.value,
        asdu_type: formAsduType.value,
        name_prefix: namePrefix.value || null,
      },
    })
    emit('added')
  } catch (e) {
    await showAlert(t('batchModal.failedPrefix', { err: String(e) }))
  } finally {
    isSaving.value = false
  }
}

function handleBackdropClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-backdrop')) {
    emit('close')
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="modal-backdrop" @click="handleBackdropClick">
      <div class="modal">
        <div class="modal-header">
          <span class="modal-title">{{ t('batchModal.title') }}</span>
          <button class="btn-close" @click="$emit('close')">×</button>
        </div>

        <div class="modal-body">
          <div class="form-row">
            <div class="form-group half">
              <label class="form-label">{{ t('batchModal.startIoa') }}</label>
              <input
                v-model.number="startIoa"
                type="number"
                class="form-input"
                min="0"
              />
            </div>
            <div class="form-group half">
              <label class="form-label">{{ t('batchModal.count') }}</label>
              <input
                v-model.number="count"
                type="number"
                class="form-input"
                min="1"
                max="100000"
              />
            </div>
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('batchModal.asduTypeLabel') }}</label>
            <select v-model="formAsduType" class="form-select">
              <option v-for="opt in ASDU_TYPES" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </option>
            </select>
          </div>

          <div class="form-group">
            <label class="form-label">{{ t('batchModal.namePrefix') }}</label>
            <input
              v-model="namePrefix"
              type="text"
              class="form-input"
              :placeholder="t('batchModal.namePrefixPlaceholder')"
            />
          </div>

          <div class="count-info">
            <span v-if="count > 100000" class="count-warn">{{ t('batchModal.countWarn') }}</span>
            <template v-else>
              <span>{{ t('batchModal.rangeHint', { startIoa, endIoa, count }) }}</span>
            </template>
          </div>
        </div>

        <div class="modal-footer">
          <button class="btn btn-secondary" @click="$emit('close')" :disabled="isSaving">{{ t('common.cancel') }}</button>
          <button class="btn btn-primary" @click="handleConfirm" :disabled="!isValid || isSaving">
            {{ isSaving ? t('batchModal.saving') : t('batchModal.add') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
}

.modal {
  background: #1e1e2e;
  border: 1px solid #45475a;
  border-radius: 8px;
  width: 420px;
  max-width: 90vw;
  max-height: 90vh;
  overflow-y: auto;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid #313244;
}

.modal-title {
  font-size: 16px;
  font-weight: 600;
  color: #cdd6f4;
}

.btn-close {
  background: none;
  border: none;
  color: #6c7086;
  font-size: 20px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
}

.btn-close:hover {
  color: #cdd6f4;
}

.modal-body {
  padding: 20px;
}

.form-row {
  display: flex;
  gap: 12px;
}

.form-group {
  margin-bottom: 16px;
}

.form-group.half {
  flex: 1;
}

.form-label {
  display: block;
  font-size: 13px;
  color: #6c7086;
  margin-bottom: 6px;
}

.form-input,
.form-select {
  width: 100%;
  padding: 8px 12px;
  background: #11111b;
  border: 1px solid #45475a;
  border-radius: 6px;
  color: #cdd6f4;
  font-size: 14px;
  box-sizing: border-box;
}

.form-input:focus,
.form-select:focus {
  outline: none;
  border-color: #89b4fa;
}

.count-info {
  font-size: 13px;
  color: #a6adc8;
  padding: 8px 0;
}

.count-info strong {
  color: #a6e3a1;
}

.count-warn {
  color: #f38ba8;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px 20px;
  border-top: 1px solid #313244;
}

.btn {
  padding: 8px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}

.btn-primary {
  background: #89b4fa;
  color: #1e1e2e;
  font-weight: 600;
}

.btn-primary:hover {
  background: #74c7ec;
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: #45475a;
  color: #cdd6f4;
}

.btn-secondary:hover {
  background: #585b70;
}

.btn-secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
