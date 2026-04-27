<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useDialogState } from '../composables/useDialog'
import { useI18n } from '../i18n'

const { state, dialogConfirm, dialogCancel } = useDialogState()
const { t } = useI18n()
const inputRef = ref<HTMLInputElement | null>(null)
const inputValue = ref('')

watch(() => state.value.visible, async (visible) => {
  if (visible && state.value.mode === 'prompt') {
    inputValue.value = state.value.defaultValue
    await nextTick()
    inputRef.value?.focus()
    inputRef.value?.select()
  }
})

function handleConfirm() {
  if (state.value.mode === 'prompt') {
    dialogConfirm(inputValue.value)
  } else {
    dialogConfirm()
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    handleConfirm()
  } else if (e.key === 'Escape') {
    dialogCancel()
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="state.visible" class="dialog-backdrop" @mousedown.self="dialogCancel" @keydown="handleKeydown">
      <div class="dialog" role="dialog" aria-modal="true">
        <div class="dialog-header">
          <span class="dialog-title">{{ state.title }}</span>
        </div>
        <div class="dialog-body">
          <p class="dialog-message">{{ state.message }}</p>
          <input
            v-if="state.mode === 'prompt'"
            ref="inputRef"
            v-model="inputValue"
            class="dialog-input"
            type="text"
            @keydown.enter="handleConfirm"
            @keydown.escape="dialogCancel"
          />
        </div>
        <div class="dialog-footer">
          <button
            v-if="state.mode !== 'alert'"
            class="btn btn-secondary"
            @click="dialogCancel"
          >{{ t('appDialog.cancel') }}</button>
          <button
            class="btn btn-primary"
            @click="handleConfirm"
          >{{ t('appDialog.ok') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
}

.dialog {
  background: #1e1e2e;
  border: 1px solid #45475a;
  border-radius: 8px;
  width: 360px;
  max-width: 90vw;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.dialog-header {
  padding: 16px 20px 0;
}

.dialog-title {
  font-size: 15px;
  font-weight: 600;
  color: #cdd6f4;
}

.dialog-body {
  padding: 12px 20px 16px;
}

.dialog-message {
  font-size: 13px;
  color: #bac2de;
  line-height: 1.5;
  margin: 0 0 8px;
  word-break: break-word;
}

.dialog-input {
  width: 100%;
  padding: 8px 12px;
  background: #11111b;
  border: 1px solid #45475a;
  border-radius: 6px;
  color: #cdd6f4;
  font-size: 14px;
  box-sizing: border-box;
  margin-top: 4px;
}

.dialog-input:focus {
  outline: none;
  border-color: #89b4fa;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 0 20px 16px;
}

.btn {
  padding: 7px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
}

.btn-primary {
  background: #89b4fa;
  color: #1e1e2e;
}

.btn-primary:hover {
  background: #74c7ec;
}

.btn-secondary {
  background: #45475a;
  color: #cdd6f4;
}

.btn-secondary:hover {
  background: #585b70;
}
</style>
