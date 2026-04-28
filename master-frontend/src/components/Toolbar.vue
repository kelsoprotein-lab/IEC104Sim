<script setup lang="ts">
import { inject, ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '../composables/useDialog'
import type { showAlert as ShowAlert } from '../composables/useDialog'
import AboutDialog from './AboutDialog.vue'
import ControlDialog from './ControlDialog.vue'
import LangSwitch from './LangSwitch.vue'
import { useI18n } from '../i18n'

const { t } = useI18n()

const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!
const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!
const selectedConnectionState = inject<Ref<string>>('selectedConnectionState')!
const refreshTree = inject<() => void>('refreshTree')!
const refreshData = inject<() => void>('refreshData')!

// About dialog
const showAbout = ref(false)

// Free-form control dialog (entry from the toolbar; no preselected point)
const showCustomControl = ref(false)
const customControlCA = ref<number>(1)
async function openCustomControl() {
  customControlCA.value = 1
  // If a connection is selected, default the dialog's CA to its first
  // configured Common Address — saves the user a step in single-CA setups
  // and gives a sensible starting point in multi-CA ones.
  if (selectedConnectionId.value) {
    try {
      const conns = await invoke<{ id: string; common_addresses: number[] }[]>('list_connections')
      const conn = conns.find((c) => c.id === selectedConnectionId.value)
      if (conn?.common_addresses?.length) customControlCA.value = conn.common_addresses[0]
    } catch { /* ignore — fall back to 1 */ }
  }
  showCustomControl.value = true
}

// New Connection modal — persist the user's last-used form values so that
// TLS paths, target address, etc. survive across app restarts.
const NEW_CONN_FORM_KEY = 'iec104master.newConnForm.v1'
type NewConnForm = {
  target_address: string
  port: number
  /** Free-form text user types: e.g. "1, 2, 3". Parsed on submit. */
  common_addresses_text: string
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
  common_addresses_text: '1',
  use_tls: false,
  ca_file: './ca.pem',
  cert_file: './client.pem',
  key_file: './client-key.pem',
  accept_invalid_certs: false,
  tls_version: 'auto',
})
function parseCAList(s: string): number[] {
  const seen = new Set<number>()
  const out: number[] = []
  for (const tok of s.split(/[,，\s]+/)) {
    if (!tok) continue
    const n = parseInt(tok, 10)
    if (!Number.isFinite(n) || n < 1 || n > 65534) continue
    if (seen.has(n)) continue
    seen.add(n); out.push(n)
  }
  return out
}
function loadForm(): NewConnForm {
  try {
    const raw = localStorage.getItem(NEW_CONN_FORM_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<NewConnForm> & { common_address?: number }
      // Migrate legacy single-CA field to text representation.
      if (typeof parsed.common_address === 'number' && parsed.common_addresses_text == null) {
        parsed.common_addresses_text = String(parsed.common_address)
      }
      delete (parsed as { common_address?: number }).common_address
      const merged = { ...defaultForm(), ...parsed } as NewConnForm
      const def = defaultForm()
      if (!merged.ca_file) merged.ca_file = def.ca_file
      if (!merged.cert_file) merged.cert_file = def.cert_file
      if (!merged.key_file) merged.key_file = def.key_file
      if (!merged.common_addresses_text) merged.common_addresses_text = def.common_addresses_text
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
  const cas = parseCAList(newConnForm.value.common_addresses_text)
  if (cas.length === 0) {
    await showAlert(t('newConn.invalidCA'))
    return
  }
  try {
    await invoke('create_connection', {
      request: {
        target_address: newConnForm.value.target_address,
        port: newConnForm.value.port,
        common_addresses: cas,
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

async function getConnCAs(): Promise<number[]> {
  const conns = await invoke<any[]>('list_connections')
  const conn = conns.find((c: any) => c.id === selectedConnectionId.value)
  const list: unknown = conn?.common_addresses
  if (Array.isArray(list) && list.length > 0) return list as number[]
  return [conn?.common_address ?? 1]
}

async function connectMaster() {
  if (!selectedConnectionId.value) return
  try {
    await invoke('connect_master', { id: selectedConnectionId.value })
    selectedConnectionState.value = 'Connected'
    refreshTree()
    try {
      const cas = await getConnCAs()
      for (const ca of cas) {
        await invoke('send_interrogation', {
          id: selectedConnectionId.value,
          commonAddress: ca,
        })
      }
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
  let alertErr: unknown = null
  try {
    await invoke('disconnect_master', { id: selectedConnectionId.value })
  } catch (e) {
    // "NotConnected" is benign: backend already saw the socket close before
    // the user clicked. For any other error we still surface it but also
    // force the UI to Disconnected so the user isn't stuck with a dead
    // button while the backend reconciles.
    const msg = String(e)
    if (!msg.includes('NotConnected') && !msg.includes('not connected')) {
      alertErr = e
    }
  } finally {
    selectedConnectionState.value = 'Disconnected'
    refreshTree()
  }
  if (alertErr !== null) {
    await showAlert(String(alertErr))
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
    const cas = await getConnCAs()
    for (const ca of cas) {
      await invoke('send_interrogation', {
        id: selectedConnectionId.value,
        commonAddress: ca,
      })
    }
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
    const cas = await getConnCAs()
    for (const ca of cas) {
      await invoke('send_clock_sync', {
        id: selectedConnectionId.value,
        commonAddress: ca,
      })
    }
  } catch (e) {
    await showAlert(String(e))
  }
}

async function sendCounterRead() {
  if (!selectedConnectionId.value) return
  try {
    const cas = await getConnCAs()
    for (const ca of cas) {
      await invoke('send_counter_read', {
        id: selectedConnectionId.value,
        commonAddress: ca,
      })
    }
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
        <span class="btn-icon">+</span> {{ t('toolbar.newConnection') }}
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn btn-start" :disabled="!hasConnection() || isConnected()" @click="connectMaster">
        {{ t('toolbar.connect') }}
      </button>
      <button class="toolbar-btn btn-stop" :disabled="!hasConnection() || !isConnected()" @click="disconnectMaster">
        {{ t('toolbar.disconnect') }}
      </button>
      <button class="toolbar-btn btn-close" :disabled="!hasConnection()" @click="deleteMaster">
        {{ t('toolbar.delete') }}
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendGI">
        {{ t('toolbar.sendGI') }}
      </button>
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendClockSync">
        {{ t('toolbar.clockSync') }}
      </button>
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendCounterRead">
        {{ t('toolbar.counterRead') }}
      </button>
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="openCustomControl">
        {{ t('toolbar.customControl') }}
      </button>
    </div>

    <div class="toolbar-spacer"></div>
    <LangSwitch />
    <button class="toolbar-title as-button" @click="showAbout = true" :title="t('toolbar.about')">
      {{ t('toolbar.appTitle') }}
    </button>
  </div>

  <AboutDialog :visible="showAbout" @close="showAbout = false" />

  <!-- Free-form control dialog. The user can pick a CA, type any IOA,
       choose a command type, and send — independent of any selected
       data point. Useful for sending control commands to IOAs that
       haven't been received yet (e.g. write-only points). -->
  <ControlDialog
    :visible="showCustomControl"
    :connection-id="selectedConnectionId"
    :common-address="customControlCA"
    :prefill-ioa="null"
    :prefill-command-type="null"
    @close="showCustomControl = false"
    @sent="showCustomControl = false"
  />

  <!-- New Connection Modal -->
  <Teleport to="body">
    <div v-if="showNewConn" class="modal-backdrop" @mousedown.self="showNewConn = false">
      <div class="modal-box">
        <div class="modal-title">{{ t('newConn.title') }}</div>
        <div class="modal-body">
          <label class="form-label">
            {{ t('newConn.targetAddress') }}
            <input v-model="newConnForm.target_address" class="form-input" type="text" placeholder="127.0.0.1" />
          </label>
          <label class="form-label">
            {{ t('newConn.port') }}
            <input v-model.number="newConnForm.port" class="form-input" type="number" min="1" max="65535" />
          </label>
          <label class="form-label">
            {{ t('newConn.commonAddress') }}
            <input
              v-model="newConnForm.common_addresses_text"
              class="form-input"
              type="text"
              placeholder="1, 2, 3"
            />
            <span class="form-hint">{{ t('newConn.commonAddressHint') }}</span>
          </label>

          <!-- TLS Configuration -->
          <div class="tls-section">
            <label class="form-label form-checkbox">
              <input type="checkbox" v-model="newConnForm.use_tls" />
              <span>{{ t('newConn.enableTls') }}</span>
            </label>
          </div>

          <template v-if="newConnForm.use_tls">
            <label class="form-label">
              {{ t('newConn.tlsVersion') }}
              <select v-model="newConnForm.tls_version" class="form-input">
                <option value="auto">{{ t('newConn.tlsAuto') }}</option>
                <option value="tls12_only">{{ t('newConn.tls12') }}</option>
                <option value="tls13_only">{{ t('newConn.tls13') }}</option>
              </select>
            </label>
            <label class="form-label">
              {{ t('newConn.caFile') }}
              <input v-model="newConnForm.ca_file" class="form-input" type="text" placeholder="/path/to/ca.crt" />
            </label>
            <label class="form-label">
              {{ t('newConn.certFile') }}
              <input v-model="newConnForm.cert_file" class="form-input" type="text" placeholder="/path/to/client.crt" />
            </label>
            <label class="form-label">
              {{ t('newConn.keyFile') }}
              <input v-model="newConnForm.key_file" class="form-input" type="text" placeholder="/path/to/client.key" />
            </label>
            <label class="form-label form-checkbox">
              <input type="checkbox" v-model="newConnForm.accept_invalid_certs" />
              <span>{{ t('newConn.acceptInvalidCerts') }}</span>
            </label>
          </template>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" @click="showNewConn = false">{{ t('common.cancel') }}</button>
          <button class="btn btn-primary" @click="createConnection">{{ t('newConn.create') }}</button>
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

.form-hint {
  font-size: 11px;
  color: #6c7086;
  margin-top: 2px;
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
