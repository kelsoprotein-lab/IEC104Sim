<script setup lang="ts">
import { inject, ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '../composables/useDialog'
import type { showAlert as ShowAlert } from '../composables/useDialog'
import AboutDialog from './AboutDialog.vue'

const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!
const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!
const selectedConnectionState = inject<Ref<string>>('selectedConnectionState')!
const refreshTree = inject<() => void>('refreshTree')!
const refreshData = inject<() => void>('refreshData')!

// About dialog
const showAbout = ref(false)

// New Connection modal — persist the user's last-used form values so that
// TLS paths, target address, etc. survive across app restarts.
const NEW_CONN_FORM_KEY = 'iec104master.newConnForm.v1'
type NewConnForm = {
  target_address: string
  port: number
  common_address: number
  use_tls: boolean
  ca_file: string
  cert_file: string
  key_file: string
  accept_invalid_certs: boolean
  tls_version: 'auto' | 'tls12_only' | 'tls13_only'
}
const defaultForm = (): NewConnForm => ({
  target_address: '127.0.0.1',
  port: 2404,
  common_address: 1,
  use_tls: false,
  ca_file: './ca.pem',
  cert_file: './client.pem',
  key_file: './client-key.pem',
  accept_invalid_certs: false,
  tls_version: 'auto',
})
function loadForm(): NewConnForm {
  try {
    const raw = localStorage.getItem(NEW_CONN_FORM_KEY)
    if (raw) {
      const merged = { ...defaultForm(), ...JSON.parse(raw) }
      const def = defaultForm()
      if (!merged.ca_file) merged.ca_file = def.ca_file
      if (!merged.cert_file) merged.cert_file = def.cert_file
      if (!merged.key_file) merged.key_file = def.key_file
      return merged
    }
  } catch {}
  return defaultForm()
}
const showNewConn = ref(false)
const newConnForm = ref<NewConnForm>(loadForm())
watch(newConnForm, (v) => {
  try { localStorage.setItem(NEW_CONN_FORM_KEY, JSON.stringify(v)) } catch {}
}, { deep: true })

async function createConnection() {
  try {
    await invoke('create_connection', {
      request: {
        target_address: newConnForm.value.target_address,
        port: newConnForm.value.port,
        common_address: newConnForm.value.common_address,
        use_tls: newConnForm.value.use_tls,
        ca_file: newConnForm.value.ca_file || undefined,
        cert_file: newConnForm.value.cert_file || undefined,
        key_file: newConnForm.value.key_file || undefined,
        accept_invalid_certs: newConnForm.value.accept_invalid_certs,
        tls_version: newConnForm.value.use_tls ? newConnForm.value.tls_version : undefined,
      }
    })
    showNewConn.value = false
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function connectMaster() {
  if (!selectedConnectionId.value) return
  try {
    await invoke('connect_master', { id: selectedConnectionId.value })
    selectedConnectionState.value = 'Connected'
    refreshTree()
    try {
      const conns = await invoke<any[]>('list_connections')
      const conn = conns.find((c: any) => c.id === selectedConnectionId.value)
      const ca = conn?.common_address ?? 1
      await invoke('send_interrogation', {
        id: selectedConnectionId.value,
        commonAddress: ca,
      })
      refreshData()
      setTimeout(() => refreshTree(), 3000)
    } catch (e) {
      console.warn('Auto GI after connect failed:', e)
    }
  } catch (e) {
    await showAlert(String(e))
  }
}

async function disconnectMaster() {
  if (!selectedConnectionId.value) return
  try {
    await invoke('disconnect_master', { id: selectedConnectionId.value })
    selectedConnectionState.value = 'Disconnected'
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function deleteMaster() {
  if (!selectedConnectionId.value) return
  try {
    await invoke('delete_connection', { id: selectedConnectionId.value })
    selectedConnectionId.value = null
    selectedConnectionState.value = 'Disconnected'
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function sendGI() {
  if (!selectedConnectionId.value) return
  try {
    const conns = await invoke<any[]>('list_connections')
    const conn = conns.find((c: any) => c.id === selectedConnectionId.value)
    const ca = conn?.common_address ?? 1
    await invoke('send_interrogation', {
      id: selectedConnectionId.value,
      commonAddress: ca,
    })
    refreshData()
    // Delayed tree refresh to update category counts after data arrives
    setTimeout(() => refreshTree(), 3000)
  } catch (e) {
    await showAlert(String(e))
  }
}

async function sendClockSync() {
  if (!selectedConnectionId.value) return
  try {
    const conns = await invoke<any[]>('list_connections')
    const conn = conns.find((c: any) => c.id === selectedConnectionId.value)
    const ca = conn?.common_address ?? 1
    await invoke('send_clock_sync', {
      id: selectedConnectionId.value,
      commonAddress: ca,
    })
  } catch (e) {
    await showAlert(String(e))
  }
}

async function sendCounterRead() {
  if (!selectedConnectionId.value) return
  try {
    const conns = await invoke<any[]>('list_connections')
    const conn = conns.find((c: any) => c.id === selectedConnectionId.value)
    const ca = conn?.common_address ?? 1
    await invoke('send_counter_read', {
      id: selectedConnectionId.value,
      commonAddress: ca,
    })
    refreshData()
    setTimeout(() => refreshTree(), 3000)
  } catch (e) {
    await showAlert(String(e))
  }
}

const isConnected = () => selectedConnectionState.value === 'Connected'
const hasConnection = () => selectedConnectionId.value !== null
</script>

<template>
  <div class="toolbar">
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="showNewConn = true">
        <span class="btn-icon">+</span> 新建连接
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn btn-start" :disabled="!hasConnection() || isConnected()" @click="connectMaster">
        连接
      </button>
      <button class="toolbar-btn btn-stop" :disabled="!hasConnection() || !isConnected()" @click="disconnectMaster">
        断开
      </button>
      <button class="toolbar-btn btn-close" :disabled="!hasConnection()" @click="deleteMaster">
        删除
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendGI">
        总召唤
      </button>
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendClockSync">
        时钟同步
      </button>
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendCounterRead">
        累计量召唤
      </button>
    </div>

    <div class="toolbar-spacer"></div>
    <button class="toolbar-title as-button" @click="showAbout = true" title="关于">
      IEC104 Master
    </button>
  </div>

  <AboutDialog :visible="showAbout" @close="showAbout = false" />

  <!-- New Connection Modal -->
  <Teleport to="body">
    <div v-if="showNewConn" class="modal-backdrop" @mousedown.self="showNewConn = false">
      <div class="modal-box">
        <div class="modal-title">新建连接</div>
        <div class="modal-body">
          <label class="form-label">
            目标地址
            <input v-model="newConnForm.target_address" class="form-input" type="text" placeholder="127.0.0.1" />
          </label>
          <label class="form-label">
            端口
            <input v-model.number="newConnForm.port" class="form-input" type="number" min="1" max="65535" />
          </label>
          <label class="form-label">
            公共地址 (CA)
            <input v-model.number="newConnForm.common_address" class="form-input" type="number" min="1" max="65534" />
          </label>

          <!-- TLS Configuration -->
          <div class="tls-section">
            <label class="form-label form-checkbox">
              <input type="checkbox" v-model="newConnForm.use_tls" />
              <span>启用 TLS</span>
            </label>
          </div>

          <template v-if="newConnForm.use_tls">
            <label class="form-label">
              TLS 版本
              <select v-model="newConnForm.tls_version" class="form-input">
                <option value="auto">自动</option>
                <option value="tls12_only">仅 TLS 1.2</option>
                <option value="tls13_only">仅 TLS 1.3</option>
              </select>
            </label>
            <label class="form-label">
              CA 证书路径
              <input v-model="newConnForm.ca_file" class="form-input" type="text" placeholder="/path/to/ca.crt" />
            </label>
            <label class="form-label">
              客户端证书路径
              <input v-model="newConnForm.cert_file" class="form-input" type="text" placeholder="/path/to/client.crt" />
            </label>
            <label class="form-label">
              客户端密钥路径
              <input v-model="newConnForm.key_file" class="form-input" type="text" placeholder="/path/to/client.key" />
            </label>
            <label class="form-label form-checkbox">
              <input type="checkbox" v-model="newConnForm.accept_invalid_certs" />
              <span>接受无效证书（测试用）</span>
            </label>
          </template>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" @click="showNewConn = false">取消</button>
          <button class="btn btn-primary" @click="createConnection">创建</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  height: 42px;
  padding: 0 8px;
  gap: 0;
}

.toolbar-group {
  display: flex;
  gap: 2px;
}

.toolbar-divider {
  width: 1px;
  height: 20px;
  background: #313244;
  margin: 0 6px;
}

.toolbar-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border: none;
  background: transparent;
  color: #cdd6f4;
  cursor: pointer;
  border-radius: 4px;
  font-size: 12px;
  white-space: nowrap;
}

.toolbar-btn:hover:not(:disabled) {
  background: #313244;
}

.toolbar-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.btn-icon {
  font-weight: bold;
  font-size: 14px;
}

.btn-start { color: #a6e3a1; }
.btn-stop { color: #fab387; }
.btn-close { color: #f38ba8; }

.toolbar-spacer {
  flex: 1;
}

.toolbar-title {
  font-size: 13px;
  font-weight: 600;
  color: #6c7086;
  padding-right: 8px;
}
.toolbar-title.as-button {
  background: transparent;
  border: none;
  cursor: pointer;
  font-family: inherit;
}
.toolbar-title.as-button:hover { color: #cdd6f4; }

/* Modal */
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
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
  min-width: 340px;
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
  gap: 12px;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
}

.form-label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: #6c7086;
}

.form-input {
  padding: 6px 10px;
  background: #313244;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 13px;
}

.form-input:focus {
  outline: none;
  border-color: #89b4fa;
}

.tls-section {
  padding-top: 4px;
  border-top: 1px solid #313244;
}

.form-checkbox {
  flex-direction: row;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  color: #cdd6f4;
  font-size: 13px;
}

.form-checkbox input[type="checkbox"] {
  width: 16px;
  height: 16px;
  accent-color: #89b4fa;
  cursor: pointer;
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
