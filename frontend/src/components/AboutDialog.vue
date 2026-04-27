<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { APP_NAME, RELEASE_NOTES, REPO_URL, RELEASES_URL } from '../releaseNotes'
import { useI18n } from '../i18n'

defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()

const { t } = useI18n()
const version = ref('')
const copied = ref('')
onMounted(async () => {
  try {
    version.value = await getVersion()
  } catch {
    version.value = ''
  }
})

async function copyLink(url: string, label: string) {
  try {
    await navigator.clipboard.writeText(url)
    copied.value = `${label} ${t('about.copiedSuffix')}`
    setTimeout(() => (copied.value = ''), 1500)
  } catch {
    copied.value = url
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="about-backdrop" @mousedown.self="emit('close')">
      <div class="about-box" role="dialog" aria-modal="true">
        <div class="about-header">
          <div class="about-title">{{ APP_NAME }}</div>
          <div class="about-version">v{{ version || '—' }}</div>
        </div>
        <div class="about-body">
          <div class="about-section-title">{{ t('about.whatsNew') }}</div>
          <ul class="about-notes">
            <li v-for="(note, i) in RELEASE_NOTES" :key="i">{{ note }}</li>
          </ul>
          <div class="about-links">
            <a href="#" @click.prevent="copyLink(REPO_URL, t('about.homepageLabel'))">{{ t('about.homepageLabel') }}</a>
            <span class="about-sep">·</span>
            <a href="#" @click.prevent="copyLink(RELEASES_URL, t('about.releasesLabel'))">{{ t('about.releasesLabel') }}</a>
            <span v-if="copied" class="about-toast">{{ copied }}</span>
          </div>
        </div>
        <div class="about-footer">
          <button class="btn btn-primary" @click="emit('close')">{{ t('common.close') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.about-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2100;
}
.about-box {
  background: #1e1e2e;
  border: 1px solid #45475a;
  border-radius: 8px;
  width: 420px;
  max-width: 90vw;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
.about-header {
  padding: 18px 22px 10px;
  border-bottom: 1px solid #313244;
}
.about-title { font-size: 16px; font-weight: 600; color: #cdd6f4; }
.about-version { font-size: 12px; color: #a6adc8; margin-top: 2px; font-variant-numeric: tabular-nums; }
.about-body { padding: 14px 22px 8px; color: #bac2de; font-size: 13px; }
.about-section-title { color: #cdd6f4; font-weight: 600; margin-bottom: 6px; }
.about-notes { margin: 0 0 14px; padding-left: 18px; line-height: 1.65; }
.about-links { font-size: 12px; display: flex; align-items: center; flex-wrap: wrap; gap: 4px; }
.about-links a { color: #89b4fa; text-decoration: none; cursor: pointer; }
.about-links a:hover { text-decoration: underline; }
.about-sep { color: #585b70; }
.about-toast { color: #a6e3a1; margin-left: 10px; font-size: 11px; }
.about-footer { display: flex; justify-content: flex-end; padding: 8px 22px 16px; }
.btn { padding: 7px 20px; border: none; border-radius: 6px; cursor: pointer; font-size: 13px; }
.btn-primary { background: #89b4fa; color: #1e1e2e; }
.btn-primary:hover { background: #74c7ec; }
</style>
