<script setup lang="ts">
import { ref, inject, watch, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '../composables/useDialog'
import type { showAlert as ShowAlert, showPrompt as ShowPrompt } from '../composables/useDialog'
import AboutDialog from './AboutDialog.vue'
import LangSwitch from './LangSwitch.vue'
import { useI18n } from '../i18n'

const { t } = useI18n()
const showAbout = ref(false)

const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedServerState = inject<Ref<string>>('selectedServerState')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const refreshTree = inject<() => void>('refreshTree')!
const refreshData = inject<() => void>('refreshData')!
const { showAlert, showPrompt } = inject<{
  showAlert: typeof ShowAlert
  showPrompt: typeof ShowPrompt
}>(dialogKey)!
const openParseFrame = inject<(prefill?: string) => void>('openParseFrame')!

type UpdateMeta = { version: string; notes: string; pub_date?: string | null }
const checkUpdate = inject<(force?: boolean) => Promise<UpdateMeta | null>>('checkUpdate')!
const updateChecking = ref(false)
async function manualCheckUpdate() {
  if (updateChecking.value) return
  updateChecking.value = true
  try {
    const meta = await checkUpdate(true)
    if (!meta) await showAlert(t('toolbar.alreadyLatest'))
  } finally {
    updateChecking.value = false
  }
}

// --- New Server Modal ---
const showNewServerModal = ref(false)
const newServerPort = ref('2404')
const newServerInitMode = ref('zero')
const newServerCount = ref(10)
const newServerUseTls = ref(false)
const newServerCertFile = ref('')
const newServerKeyFile = ref('')
const newServerCaFile = ref('')
const newServerRequireClientCert = ref(false)

function openNewServerModal() {
  newServerPort.value = '2404'
  newServerInitMode.value = 'zero'
  newServerCount.value = 10
  newServerUseTls.value = false
  newServerCertFile.value = ''
  newServerKeyFile.value = ''
  newServerCaFile.value = ''
  newServerRequireClientCert.value = false
  showNewServerModal.value = true
}

async function submitNewServer() {
  const port = Number(newServerPort.value)
  if (!port || port < 1 || port > 65535) {
    await showAlert(t('errors.invalidPort'))
    return
  }
  showNewServerModal.value = false
  try {
    const count = Number.isFinite(newServerCount.value) && newServerCount.value >= 0
      ? Math.min(65534, Math.floor(newServerCount.value))
      : 10
    const info = await invoke<{ id: string }>('create_server', {
      request: {
        port,
        init_mode: newServerInitMode.value,
        count_per_category: count,
        use_tls: newServerUseTls.value || undefined,
        cert_file: newServerCertFile.value || undefined,
        key_file: newServerKeyFile.value || undefined,
        ca_file: newServerCaFile.value || undefined,
        require_client_cert: newServerRequireClientCert.value || undefined,
      },
    })
    await invoke('start_server', { id: info.id })
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

// --- Start / Stop ---
async function startServer() {
  if (!selectedServerId.value) return
  try {
    await invoke('start_server', { id: selectedServerId.value })
    selectedServerState.value = 'Running'
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function stopServer() {
  if (!selectedServerId.value) return
  try {
    await invoke('stop_server', { id: selectedServerId.value })
    selectedServerState.value = 'Stopped'
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

// --- Add Station ---
async function addStation() {
  if (!selectedServerId.value) return
  const caStr = await showPrompt(t('prompt.inputCommonAddress'), '1')
  if (caStr === null) return
  const ca = Number(caStr)
  if (isNaN(ca) || ca < 1 || ca > 65534) {
    await showAlert(t('errors.invalidCa'))
    return
  }
  const defaultName = t('station.defaultName', { ca })
  const name = await showPrompt(t('prompt.inputStationName'), defaultName)
  if (name === null) return
  try {
    await invoke('add_station', {
      request: {
        server_id: selectedServerId.value,
        common_address: ca,
        name: name || '',
      },
    })
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

// --- Random Mutation ---
const mutationActive = ref(false)
const mutationRate = ref(1000)
let mutationTimer: number | null = null

function toggleMutation() {
  if (mutationActive.value) {
    stopMutation()
  } else {
    startMutation()
  }
}

function startMutation() {
  if (!selectedServerId.value || selectedCA.value === null) return
  mutationActive.value = true
  scheduleMutation()
}

function stopMutation() {
  mutationActive.value = false
  if (mutationTimer !== null) {
    clearTimeout(mutationTimer)
    mutationTimer = null
  }
}

function scheduleMutation() {
  if (!mutationActive.value) return
  mutationTimer = window.setTimeout(async () => {
    if (!mutationActive.value || !selectedServerId.value || selectedCA.value === null) {
      stopMutation()
      return
    }
    try {
      await invoke('random_mutate_data_points', {
        request: {
          server_id: selectedServerId.value,
          common_address: selectedCA.value,
        },
      })
      refreshData()
    } catch (e) {
      console.error('mutation failed:', e)
    }
    scheduleMutation()
  }, mutationRate.value)
}

watch([selectedServerId, selectedCA], () => {
  if (mutationActive.value) stopMutation()
})

onUnmounted(() => {
  if (mutationTimer !== null) clearTimeout(mutationTimer)
})

// --- Cyclic Transmission ---
const cyclicActive = ref(false)
const cyclicInterval = ref(2000)

async function toggleCyclic() {
  if (!selectedServerId.value || selectedCA.value === null) return
  cyclicActive.value = !cyclicActive.value
  try {
    await invoke('set_cyclic_config', {
      request: {
        server_id: selectedServerId.value,
        common_address: selectedCA.value,
        enabled: cyclicActive.value,
        interval_ms: cyclicInterval.value,
      },
    })
  } catch (e) {
    await showAlert(String(e))
    cyclicActive.value = false
  }
}

watch([selectedServerId, selectedCA], () => {
  cyclicActive.value = false
})
</script>

<template>
  <div class="toolbar">
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="openNewServerModal" :title="t('toolbar.titleNewServer')">
        <span class="toolbar-icon">+</span>
        <span class="toolbar-label">{{ t('toolbar.newServer') }}</span>
      </button>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button
        class="toolbar-btn btn-start"
        @click="startServer"
        :disabled="!selectedServerId || selectedServerState === 'Running'"
        :title="t('toolbar.titleStartServer')"
      >
        <span class="toolbar-label">{{ t('toolbar.start') }}</span>
      </button>
      <button
        class="toolbar-btn btn-stop"
        @click="stopServer"
        :disabled="!selectedServerId || selectedServerState === 'Stopped'"
        :title="t('toolbar.titleStopServer')"
      >
        <span class="toolbar-label">{{ t('toolbar.stop') }}</span>
      </button>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button
        class="toolbar-btn"
        @click="addStation"
        :disabled="!selectedServerId"
        :title="t('toolbar.titleAddStation')"
      >
        <span class="toolbar-label">{{ t('toolbar.addStation') }}</span>
      </button>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group interval-group">
      <button
        :class="['toolbar-btn', { 'btn-mutation-active': mutationActive }]"
        @click="toggleMutation"
        :disabled="!selectedServerId || selectedCA === null"
        :title="t('toolbar.titleRandomMutation')"
      >
        <span class="toolbar-label">{{ mutationActive ? t('toolbar.stopMutation') : t('toolbar.randomMutation') }}</span>
      </button>
      <input
        type="number"
        class="interval-input"
        min="100"
        max="60000"
        step="100"
        v-model.number="mutationRate"
        :title="t('toolbar.mutationInterval')"
      />
      <span class="rate-label">ms</span>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group interval-group">
      <button
        :class="['toolbar-btn', { 'btn-cyclic-active': cyclicActive }]"
        @click="toggleCyclic"
        :disabled="!selectedServerId || selectedCA === null"
        :title="t('toolbar.titleCyclicSend')"
      >
        <span class="toolbar-label">{{ cyclicActive ? t('toolbar.stopCyclic') : t('toolbar.cyclicSend') }}</span>
      </button>
      <input
        type="number"
        class="interval-input"
        min="100"
        max="60000"
        step="100"
        v-model.number="cyclicInterval"
        :title="t('toolbar.sendInterval')"
      />
      <span class="rate-label">ms</span>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="openParseFrame()" :title="t('toolbar.parseFrame')">
        <span class="toolbar-label">{{ t('toolbar.parseFrame') }}</span>
      </button>
    </div>
    <button class="toolbar-btn toolbar-btn-update" :disabled="updateChecking" @click="manualCheckUpdate">
      {{ updateChecking ? t('toolbar.checkingUpdate') : t('toolbar.checkUpdate') }}
    </button>
    <LangSwitch />
    <button class="toolbar-title as-button" @click="showAbout = true" :title="t('toolbar.about')">{{ t('toolbar.appTitle') }}</button>
  </div>

  <AboutDialog :visible="showAbout" @close="showAbout = false" />

  <!-- New Server Modal -->
  <Teleport to="body">
    <div v-if="showNewServerModal" class="modal-overlay" @mousedown.self="showNewServerModal = false">
      <div class="modal-box">
        <div class="modal-title">{{ t('newServer.title') }}</div>
        <div class="modal-field">
          <label>{{ t('newServer.portLabel') }}</label>
          <input
            v-model="newServerPort"
            type="number"
            min="1"
            max="65535"
            @keyup.enter="submitNewServer"
          />
        </div>
        <div class="modal-field">
          <label>{{ t('newServer.initMode') }}</label>
          <div class="radio-group">
            <label class="radio-label">
              <input type="radio" v-model="newServerInitMode" value="zero" /> {{ t('newServer.initZero') }}
            </label>
            <label class="radio-label">
              <input type="radio" v-model="newServerInitMode" value="random" /> {{ t('newServer.initRandom') }}
            </label>
          </div>
        </div>
        <div class="modal-field">
          <label>{{ t('newServer.countPerCategory') }}</label>
          <input
            v-model.number="newServerCount"
            type="number"
            min="0"
            max="65534"
            @keyup.enter="submitNewServer"
          />
        </div>
        <div class="modal-field">
          <label class="checkbox-label">
            <input type="checkbox" v-model="newServerUseTls" /> {{ t('newServer.enableTls') }}
          </label>
        </div>
        <template v-if="newServerUseTls">
          <div class="modal-field">
            <label>{{ t('newServer.serverCert') }}</label>
            <input
              v-model="newServerCertFile"
              type="text"
              placeholder="/path/to/server.crt"
            />
          </div>
          <div class="modal-field">
            <label>{{ t('newServer.serverKey') }}</label>
            <input
              v-model="newServerKeyFile"
              type="text"
              placeholder="/path/to/server.key"
            />
          </div>
          <div class="modal-field">
            <label>{{ t('newServer.caFile') }}</label>
            <input
              v-model="newServerCaFile"
              type="text"
              placeholder="/path/to/ca.crt"
            />
          </div>
          <div class="modal-field">
            <label class="checkbox-label">
              <input type="checkbox" v-model="newServerRequireClientCert" /> {{ t('newServer.requireClientCert') }}
            </label>
          </div>
        </template>
        <div class="modal-actions">
          <button class="modal-btn cancel" @click="showNewServerModal = false">{{ t('common.cancel') }}</button>
          <button class="modal-btn confirm" @click="submitNewServer">{{ t('common.ok') }}</button>
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
  gap: 6px;
  user-select: none;
  font-size: 13px;
}

.toolbar-group {
  display: flex;
  gap: 2px;
}

.toolbar-divider {
  width: 1px;
  height: 24px;
  background: #313244;
  margin: 0 4px;
}

.toolbar-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border: none;
  background: #313244;
  color: #cdd6f4;
  cursor: pointer;
  border-radius: 4px;
  font-size: 13px;
  white-space: nowrap;
}

.toolbar-btn:hover:not(:disabled) {
  background: #45475a;
}

.toolbar-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.toolbar-btn.btn-start:not(:disabled) {
  color: #a6e3a1;
}

.toolbar-btn.btn-stop:not(:disabled) {
  color: #fab387;
}

.toolbar-icon {
  font-weight: bold;
  font-size: 14px;
}

.toolbar-btn.btn-mutation-active {
  background: #a6e3a1;
  color: #1e1e2e;
  font-weight: 600;
}

.toolbar-btn.btn-mutation-active:hover {
  background: #94e2d5;
}

.toolbar-btn.btn-cyclic-active {
  background: #cba6f7;
  color: #1e1e2e;
  font-weight: 600;
}

.toolbar-btn.btn-cyclic-active:hover {
  background: #b4befe;
}

.interval-group {
  align-items: center;
}

.interval-input {
  width: 60px;
  padding: 2px 4px;
  background: #313244;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 11px;
  font-family: 'SF Mono', 'Fira Code', monospace;
  text-align: center;
  -moz-appearance: textfield;
}

.interval-input::-webkit-inner-spin-button,
.interval-input::-webkit-outer-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

.interval-input:focus {
  outline: none;
  border-color: #89b4fa;
}

.rate-label {
  font-size: 10px;
  color: #6c7086;
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.toolbar-btn-update {
  margin-left: auto;
}

.toolbar-title {
  font-size: 12px;
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

/* Modal styles */
.modal-overlay {
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
  min-width: 300px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-title {
  font-size: 14px;
  font-weight: 600;
  color: #cdd6f4;
  margin-bottom: 16px;
}

.modal-field {
  margin-bottom: 14px;
}

.modal-field label {
  display: block;
  font-size: 12px;
  color: #a6adc8;
  margin-bottom: 6px;
}

.modal-field input[type="number"],
.modal-field input[type="text"] {
  width: 100%;
  padding: 6px 10px;
  background: #313244;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 13px;
  outline: none;
  box-sizing: border-box;
}

.modal-field input[type="number"]:focus,
.modal-field input[type="text"]:focus {
  border-color: #89b4fa;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: #cdd6f4;
  cursor: pointer;
}

.checkbox-label input[type="checkbox"] {
  accent-color: #89b4fa;
}

.radio-group {
  display: flex;
  gap: 16px;
}

.radio-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: #cdd6f4;
  cursor: pointer;
}

.radio-label input[type="radio"] {
  accent-color: #89b4fa;
}

.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
}

.modal-btn {
  padding: 6px 16px;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.modal-btn.cancel {
  background: #313244;
  color: #a6adc8;
}

.modal-btn.cancel:hover {
  background: #45475a;
}

.modal-btn.confirm {
  background: #89b4fa;
  color: #1e1e2e;
  font-weight: 600;
}

.modal-btn.confirm:hover {
  background: #74c7ec;
}
</style>
