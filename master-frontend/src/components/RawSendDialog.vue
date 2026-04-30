<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { RawSendResult } from '../types'

interface Props {
  visible: boolean
  connectionId: string | null
}

const props = defineProps<Props>()
const emit = defineEmits<{ (e: 'close'): void }>()

const hexInput = ref('')
const sending = ref(false)
const errorMsg = ref('')
const lastResult = ref<RawSendResult | null>(null)
const previewMsg = ref('')

const TEMPLATES: { label: string; hex: string }[] = [
  { label: 'STARTDT act', hex: '68 04 07 00 00 00' },
  { label: 'STARTDT con', hex: '68 04 0B 00 00 00' },
  { label: 'STOPDT act',  hex: '68 04 13 00 00 00' },
  { label: 'TESTFR act',  hex: '68 04 43 00 00 00' },
  { label: 'TESTFR con',  hex: '68 04 83 00 00 00' },
  { label: 'S-frame (RSN=0)', hex: '68 04 01 00 00 00' },
  { label: '总召唤 act',   hex: '68 0E 00 00 00 00 64 01 06 00 01 00 00 00 00 14' },
]

watch(() => props.visible, (v) => {
  if (v) {
    errorMsg.value = ''
    lastResult.value = null
    previewMsg.value = ''
  }
})

function applyTemplate(hex: string) {
  hexInput.value = hex
  errorMsg.value = ''
  preview()
}

const compactHex = computed(() => {
  let out = ''
  for (const c of hexInput.value) {
    if (/[0-9a-fA-F]/.test(c)) out += c
    else if (/\s|,|-|:/.test(c)) continue
    else return null
  }
  return out
})

function preview() {
  const h = compactHex.value
  if (h === null) {
    previewMsg.value = '包含非法字符'
    return
  }
  if (h.length === 0) {
    previewMsg.value = '为空'
    return
  }
  if (h.length % 2 !== 0) {
    previewMsg.value = `奇数位 (${h.length} 位),需偶数`
    return
  }
  const bytes: number[] = []
  for (let i = 0; i < h.length; i += 2) {
    bytes.push(parseInt(h.slice(i, i + 2), 16))
  }
  if (bytes.length < 6 || bytes[0] !== 0x68) {
    previewMsg.value = `${bytes.length} 字节,首字节 0x${bytes[0]?.toString(16).toUpperCase().padStart(2, '0') ?? '??'} (合规需 ≥6 且首字节 0x68)`
    return
  }
  const declared = bytes[1]
  const expected = declared + 2
  const ctrl1 = bytes[2]
  let kind = 'I 帧'
  if ((ctrl1 & 0x03) === 0x03) kind = 'U 帧'
  else if ((ctrl1 & 0x03) === 0x01) kind = 'S 帧'
  const lenOk = expected === bytes.length
  previewMsg.value = `${kind},LEN=${declared} (期望总长 ${expected}/实际 ${bytes.length}) ${lenOk ? '✓' : '✗'}`
}

async function send() {
  if (!props.connectionId) {
    errorMsg.value = '未选择连接'
    return
  }
  errorMsg.value = ''
  lastResult.value = null
  sending.value = true
  try {
    const result = await invoke<RawSendResult>('send_raw_apdu', {
      request: {
        connection_id: props.connectionId,
        hex_payload: hexInput.value,
      }
    })
    lastResult.value = result
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    sending.value = false
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="modal-backdrop" @mousedown.self="emit('close')" @keydown="handleKeydown">
      <div class="modal-box">
        <div class="modal-title">原始报文发送</div>
        <div class="modal-body">
          <div class="hint">
            发送任意 APDU。I 帧的 SSN/RSN 与 S 帧的 RSN 会被自动覆写为当前会话计数;
            U 帧 (STARTDT/STOPDT/TESTFR) 原样透传。
          </div>

          <label class="form-label">
            十六进制字节 (允许空格/换行/逗号)
            <textarea v-model="hexInput" @input="preview" class="hex-area" rows="4"
              placeholder="68 04 07 00 00 00" spellcheck="false"></textarea>
          </label>

          <div class="preview-row">
            <button class="btn btn-secondary btn-sm" type="button" @click="preview">解析预览</button>
            <span class="preview-msg">{{ previewMsg || '—' }}</span>
          </div>

          <div class="templates">
            <span class="templates-label">模板:</span>
            <button v-for="t in TEMPLATES" :key="t.label" type="button"
              class="template-btn" @click="applyTemplate(t.hex)">{{ t.label }}</button>
          </div>

          <div v-if="errorMsg" class="error-msg">{{ errorMsg }}</div>
          <div v-if="lastResult" class="result-ok">
            <div class="result-line"><span class="k">已发送:</span><span class="v">{{ lastResult.byte_len }} 字节 @ {{ lastResult.timestamp }}</span></div>
            <div class="result-bytes">{{ lastResult.sent_hex }}</div>
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" @click="emit('close')">关闭</button>
          <button class="btn btn-primary" :disabled="sending || !connectionId" @click="send">
            {{ sending ? '发送中...' : '发送' }}
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
  z-index: 1000;
}

.modal-box {
  background: #1e1e2e;
  border: 1px solid #45475a;
  border-radius: 8px;
  padding: 20px;
  min-width: 480px;
  max-width: 90vw;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-title {
  font-size: 15px;
  font-weight: 600;
  color: #cdd6f4;
  margin-bottom: 16px;
}

.modal-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}

.hint {
  font-size: 11px;
  color: #6c7086;
  line-height: 1.5;
}

.form-label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: #6c7086;
}

.hex-area {
  padding: 8px 10px;
  background: #313244;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 12px;
  resize: vertical;
}

.hex-area:focus {
  outline: none;
  border-color: #89b4fa;
}

.preview-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.preview-msg {
  font-size: 11px;
  color: #cdd6f4;
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.templates {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.templates-label {
  font-size: 11px;
  color: #6c7086;
}

.template-btn {
  padding: 3px 8px;
  font-size: 11px;
  background: #313244;
  border: 1px solid #45475a;
  color: #cdd6f4;
  border-radius: 4px;
  cursor: pointer;
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.template-btn:hover {
  background: #45475a;
  border-color: #89b4fa;
}

.error-msg {
  padding: 8px 10px;
  background: rgba(243, 139, 168, 0.15);
  border: 1px solid #f38ba8;
  border-radius: 4px;
  color: #f38ba8;
  font-size: 12px;
  word-break: break-word;
}

.result-ok {
  padding: 8px 10px;
  background: rgba(166, 227, 161, 0.12);
  border: 1px solid rgba(166, 227, 161, 0.35);
  border-radius: 4px;
  color: #a6e3a1;
  font-size: 11px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.result-line .k { color: #6c7086; margin-right: 6px; }
.result-line .v { font-family: 'SF Mono', 'Fira Code', monospace; }
.result-bytes {
  font-family: 'SF Mono', 'Fira Code', monospace;
  word-break: break-all;
  color: #cdd6f4;
}

.btn {
  padding: 7px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
}

.btn-sm {
  padding: 4px 10px;
  font-size: 11px;
}

.btn-primary {
  background: #89b4fa;
  color: #1e1e2e;
  font-weight: 600;
}

.btn-primary:hover:not(:disabled) { background: #74c7ec; }
.btn-primary:disabled { opacity: 0.5; cursor: default; }

.btn-secondary {
  background: #45475a;
  color: #cdd6f4;
}

.btn-secondary:hover { background: #585b70; }
</style>
