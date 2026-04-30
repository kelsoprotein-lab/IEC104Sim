<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ParsedFrame, ParsedObject } from '../types'

interface Props {
  visible: boolean
  prefill?: string
}

const props = defineProps<Props>()
const emit = defineEmits<{ (e: 'close'): void }>()

const hexInput = ref('')
const parsing = ref(false)
const errorMsg = ref('')
const result = ref<ParsedFrame | null>(null)

const TEMPLATES: { label: string; hex: string }[] = [
  { label: 'STARTDT act', hex: '68 04 07 00 00 00' },
  { label: 'STARTDT con', hex: '68 04 0B 00 00 00' },
  { label: 'TESTFR act',  hex: '68 04 43 00 00 00' },
  { label: 'S 帧 RSN=0',   hex: '68 04 01 00 00 00' },
  { label: '总召唤 act',   hex: '68 0E 00 00 00 00 64 01 06 00 01 00 00 00 00 14' },
  { label: 'M_ME_NC_1',    hex: '68 10 00 00 00 00 0D 01 03 00 01 00 01 00 00 00 00 C0 3F 00' },
]

watch(() => props.visible, (v) => {
  if (v) {
    errorMsg.value = ''
    result.value = null
    if (props.prefill) {
      hexInput.value = props.prefill
      parse()
    }
  }
})

function applyTemplate(hex: string) {
  hexInput.value = hex
  errorMsg.value = ''
  parse()
}

function clear() {
  hexInput.value = ''
  errorMsg.value = ''
  result.value = null
}

async function parse() {
  if (!hexInput.value.trim()) {
    errorMsg.value = '请输入 hex 报文'
    return
  }
  errorMsg.value = ''
  result.value = null
  parsing.value = true
  try {
    result.value = await invoke<ParsedFrame>('parse_frame_full', { data: hexInput.value })
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    parsing.value = false
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}

const apciKindLabel = computed(() => {
  if (!result.value) return ''
  const a = result.value.apci
  if (a.frame_type === 'i') return 'I 帧 (Information)'
  if (a.frame_type === 's') return 'S 帧 (Supervisory)'
  return `U 帧 · ${a.name}`
})

const apciKindClass = computed(() => {
  if (!result.value) return ''
  return `kind-${result.value.apci.frame_type}`
})

function hex2(b: number): string {
  return b.toString(16).toUpperCase().padStart(2, '0')
}

function formatValue(obj: ParsedObject): string {
  if (!obj.value) return '—'
  const v = obj.value as Record<string, unknown>
  switch (v.type) {
    case 'single_point':     return v.value ? 'ON' : 'OFF'
    case 'double_point':     {
      const n = v.value as number
      return ['中间', 'OFF', 'ON', '不确定'][n] ?? String(n)
    }
    case 'step_position':    return `${v.value}${v.transient ? ' (T)' : ''}`
    case 'bitstring':        return `0x${(v.value as number).toString(16).toUpperCase().padStart(8, '0')}`
    case 'normalized':       return (v.value as number).toFixed(4)
    case 'scaled':           return String(v.value)
    case 'short_float':      return (v.value as number).toFixed(3)
    case 'integrated_total': {
      let s = String(v.value)
      if (v.carry) s += ' [C]'
      if (v.sequence) s += ` S${v.sequence}`
      return s
    }
    default: return JSON.stringify(v)
  }
}

function formatQuality(q: ParsedObject['quality']): string {
  if (!q) return ''
  const flags: string[] = []
  if (q.iv) flags.push('IV')
  if (q.nt) flags.push('NT')
  if (q.sb) flags.push('SB')
  if (q.bl) flags.push('BL')
  if (q.ov) flags.push('OV')
  return flags.length ? flags.join('|') : 'GOOD'
}

function formatTimestamp(t: ParsedObject['timestamp']): string {
  if (!t) return ''
  const pad = (n: number, w = 2) => String(n).padStart(w, '0')
  return `${t.year}-${pad(t.month)}-${pad(t.day)} ${pad(t.hour)}:${pad(t.minute)}:${pad(Math.floor(t.millisecond / 1000))}.${pad(t.millisecond % 1000, 3)}${t.invalid ? ' [IV]' : ''}`
}

const hasTimestamp = computed(() => {
  return result.value?.asdu?.objects.some(o => o.timestamp) ?? false
})
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="modal-backdrop" @mousedown.self="emit('close')" @keydown="handleKeydown">
      <div class="modal-box">
        <div class="modal-title">报文解析器</div>
        <div class="modal-body">
          <div class="hint">
            粘贴一段 IEC 60870-5-104 APDU 的十六进制字节,自动展开 APCI/ASDU/IOA 详情。
            支持空格、换行、逗号分隔。
          </div>

          <label class="form-label">
            十六进制字节
            <textarea v-model="hexInput" class="hex-area" rows="3"
              placeholder="68 0E 00 00 00 00 64 01 06 00 01 00 00 00 00 14"
              spellcheck="false" @keydown.ctrl.enter.prevent="parse"
              @keydown.meta.enter.prevent="parse"></textarea>
          </label>

          <div class="templates">
            <span class="templates-label">模板:</span>
            <button v-for="t in TEMPLATES" :key="t.label" type="button"
              class="template-btn" @click="applyTemplate(t.hex)">{{ t.label }}</button>
          </div>

          <div v-if="errorMsg" class="error-msg">{{ errorMsg }}</div>

          <template v-if="result">
            <div v-if="result.warnings.length" class="warn-msg">
              <div v-for="(w, i) in result.warnings" :key="i">⚠ {{ w }}</div>
            </div>

            <!-- APCI section -->
            <section class="card">
              <div class="card-title">
                <span class="kind-chip" :class="apciKindClass">{{ apciKindLabel }}</span>
                <span class="card-meta">{{ result.length }} 字节</span>
              </div>
              <table class="kv">
                <tbody>
                  <tr><th>起始字节</th><td><code>0x{{ hex2(result.start_byte) }}</code></td></tr>
                  <tr><th>APDU 长度</th><td><code>{{ result.apdu_length }} (0x{{ hex2(result.apdu_length) }})</code></td></tr>
                  <tr><th>控制字段</th><td><code>{{ result.control_field.map(hex2).join(' ') }}</code></td></tr>
                  <tr v-if="result.apci.frame_type === 'i'">
                    <th>序列号</th>
                    <td><code>SSN={{ result.apci.send_seq }} RSN={{ result.apci.recv_seq }}</code></td>
                  </tr>
                  <tr v-else-if="result.apci.frame_type === 's'">
                    <th>序列号</th>
                    <td><code>RSN={{ result.apci.recv_seq }}</code></td>
                  </tr>
                </tbody>
              </table>
            </section>

            <!-- ASDU section -->
            <section v-if="result.asdu" class="card">
              <div class="card-title">
                ASDU
                <span class="card-meta">{{ result.asdu.type_name }} (Type {{ result.asdu.type_id }})</span>
              </div>
              <table class="kv">
                <tbody>
                  <tr>
                    <th>类型</th>
                    <td><code>{{ result.asdu.type_id }}</code> · {{ result.asdu.type_name }}</td>
                  </tr>
                  <tr>
                    <th>VSQ</th>
                    <td>SQ={{ result.asdu.sq ? 1 : 0 }}, N={{ result.asdu.num_objects }}</td>
                  </tr>
                  <tr>
                    <th>COT</th>
                    <td>
                      <code>{{ result.asdu.cot }}</code> · {{ result.asdu.cot_name }}
                      <span v-if="result.asdu.negative" class="flag-neg">P/N=否定</span>
                      <span v-if="result.asdu.test" class="flag-test">T=测试</span>
                    </td>
                  </tr>
                  <tr><th>OA (源地址)</th><td><code>{{ result.asdu.originator }}</code></td></tr>
                  <tr><th>CA (公共地址)</th><td><code>{{ result.asdu.common_address }}</code></td></tr>
                </tbody>
              </table>
            </section>

            <!-- Objects section -->
            <section v-if="result.asdu && result.asdu.objects.length" class="card">
              <div class="card-title">
                信息对象
                <span class="card-meta">{{ result.asdu.objects.length }} 个</span>
              </div>
              <table class="objs">
                <thead>
                  <tr>
                    <th>IOA</th>
                    <th>值</th>
                    <th>品质</th>
                    <th v-if="hasTimestamp">时间戳</th>
                    <th>原始字节</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="(obj, i) in result.asdu.objects" :key="i">
                    <td><code>{{ obj.ioa }}</code></td>
                    <td>{{ formatValue(obj) }}</td>
                    <td><code class="q">{{ formatQuality(obj.quality) }}</code></td>
                    <td v-if="hasTimestamp"><code>{{ formatTimestamp(obj.timestamp) }}</code></td>
                    <td><code class="raw">{{ obj.raw_hex }}</code></td>
                  </tr>
                </tbody>
              </table>
            </section>
          </template>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" @click="clear">清空</button>
          <button class="btn btn-secondary" @click="emit('close')">关闭</button>
          <button class="btn btn-primary" :disabled="parsing" @click="parse">
            {{ parsing ? '解析中...' : '解析 (Ctrl+Enter)' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-backdrop {
  position: fixed; inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex; align-items: center; justify-content: center;
  z-index: 1000;
}
.modal-box {
  background: #1e1e2e;
  border: 1px solid #45475a;
  border-radius: 8px;
  padding: 20px;
  min-width: 640px;
  max-width: 90vw;
  max-height: 90vh;
  overflow-y: auto;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
.modal-title {
  font-size: 15px;
  font-weight: 600;
  color: #cdd6f4;
  margin-bottom: 16px;
}
.modal-body { display: flex; flex-direction: column; gap: 10px; }
.modal-footer { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
.hint { font-size: 11px; color: #6c7086; line-height: 1.5; }
.form-label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: #6c7086; }
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
.hex-area:focus { outline: none; border-color: #89b4fa; }
.templates { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
.templates-label { font-size: 11px; color: #6c7086; }
.template-btn {
  padding: 3px 8px; font-size: 11px;
  background: #313244; border: 1px solid #45475a;
  color: #cdd6f4; border-radius: 4px; cursor: pointer;
  font-family: 'SF Mono', 'Fira Code', monospace;
}
.template-btn:hover { background: #45475a; border-color: #89b4fa; }
.error-msg {
  padding: 8px 10px;
  background: rgba(243, 139, 168, 0.15);
  border: 1px solid #f38ba8;
  border-radius: 4px;
  color: #f38ba8;
  font-size: 12px;
  word-break: break-word;
}
.warn-msg {
  padding: 8px 10px;
  background: rgba(249, 226, 175, 0.12);
  border: 1px solid rgba(249, 226, 175, 0.4);
  border-radius: 4px;
  color: #f9e2af;
  font-size: 11px;
  display: flex; flex-direction: column; gap: 2px;
}
.card {
  background: #181825;
  border: 1px solid #313244;
  border-radius: 6px;
  padding: 10px 12px;
  display: flex; flex-direction: column; gap: 6px;
}
.card-title {
  display: flex; align-items: center; gap: 10px;
  font-size: 12px; font-weight: 600; color: #cdd6f4;
}
.card-meta { color: #6c7086; font-weight: 400; font-size: 11px; }
.kind-chip {
  padding: 2px 8px;
  border-radius: 10px;
  font-size: 11px;
  font-weight: 600;
}
.kind-i { background: rgba(137, 180, 250, 0.2); color: #89b4fa; }
.kind-s { background: rgba(249, 226, 175, 0.2); color: #f9e2af; }
.kind-u { background: rgba(166, 227, 161, 0.2); color: #a6e3a1; }
.kv { width: 100%; border-collapse: collapse; font-size: 12px; }
.kv th {
  text-align: left; color: #6c7086; font-weight: 400;
  padding: 3px 8px 3px 0; width: 110px; vertical-align: top;
}
.kv td { color: #cdd6f4; padding: 3px 0; }
.kv code, .objs code, .raw { font-family: 'SF Mono', 'Fira Code', monospace; }
.flag-neg { color: #f38ba8; margin-left: 6px; font-size: 10px; }
.flag-test { color: #f9e2af; margin-left: 6px; font-size: 10px; }
.objs {
  width: 100%; border-collapse: collapse; font-size: 11px;
}
.objs th {
  text-align: left; color: #6c7086; font-weight: 400;
  padding: 4px 8px; border-bottom: 1px solid #313244;
}
.objs td {
  color: #cdd6f4; padding: 3px 8px;
  border-bottom: 1px solid rgba(49, 50, 68, 0.4);
}
.objs td.q, .q { color: #a6e3a1; }
.raw { color: #6c7086; font-size: 10px; }
.btn {
  padding: 7px 20px; border: none;
  border-radius: 6px; cursor: pointer; font-size: 13px;
}
.btn-primary { background: #89b4fa; color: #1e1e2e; font-weight: 600; }
.btn-primary:hover:not(:disabled) { background: #74c7ec; }
.btn-primary:disabled { opacity: 0.5; cursor: default; }
.btn-secondary { background: #45475a; color: #cdd6f4; }
.btn-secondary:hover { background: #585b70; }
</style>
