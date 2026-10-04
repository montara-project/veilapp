import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import './settings.css'

// Same shape as the Rust `Settings`.
interface Settings {
  showMemoryUsage: boolean
}

function initTabs(): void {
  const tabs = document.querySelectorAll<HTMLButtonElement>('.tab')
  const panes = document.querySelectorAll<HTMLElement>('.pane')
  for (const tab of tabs) {
    tab.addEventListener('click', () => {
      for (const t of tabs) {
        const active = t === tab
        t.classList.toggle('active', active)
        t.setAttribute('aria-selected', String(active))
      }
      for (const pane of panes) {
        pane.classList.toggle('active', pane.id === tab.dataset.tab)
      }
    })
  }
}

async function initLaunchAtLogin(): Promise<void> {
  const toggle = document.querySelector<HTMLInputElement>('#launch-at-login')!
  try {
    toggle.checked = await isEnabled()
  } catch (err) {
    console.error('autostart isEnabled failed', err)
  }
  toggle.addEventListener('change', async () => {
    try {
      if (toggle.checked) await enable()
      else await disable()
    } catch (err) {
      console.error('autostart toggle failed', err)
      toggle.checked = !toggle.checked
    }
  })
}

async function initShowMemory(): Promise<void> {
  const toggle = document.querySelector<HTMLInputElement>('#show-memory')!
  try {
    toggle.checked = (await invoke<Settings>('get_settings')).showMemoryUsage
  } catch (err) {
    console.error('get_settings failed', err)
  }
  toggle.addEventListener('change', async () => {
    try {
      await invoke('set_show_memory_usage', { show: toggle.checked })
    } catch (err) {
      console.error('set_show_memory_usage failed', err)
      toggle.checked = !toggle.checked
    }
  })
}

async function initVersion(): Promise<void> {
  const el = document.querySelector<HTMLSpanElement>('#app-version')!
  el.textContent = await getVersion()
}

// Updater: silent check at startup, manual check via the About tab, then
// download + relaunch. Dev builds skip both — they have no matching release.
const updateStatus = document.querySelector<HTMLSpanElement>('#update-status')!
const updateDetail = document.querySelector<HTMLSpanElement>('#update-detail')!
const updateButton = document.querySelector<HTMLButtonElement>('#update-button')!

let pendingUpdate: Update | null = null

function setUpdateUI(status: string, detail: string, button: string | null): void {
  updateStatus.textContent = status
  updateDetail.textContent = detail
  if (button === null) {
    updateButton.hidden = true
  } else {
    updateButton.hidden = false
    updateButton.textContent = button
  }
}

async function checkForUpdate(manual: boolean): Promise<void> {
  try {
    if (manual) {
      updateButton.disabled = true
      setUpdateUI('Checking for updates…', '', 'Checking…')
    }
    pendingUpdate = await check()
    if (pendingUpdate) {
      setUpdateUI(`Veil App ${pendingUpdate.version} is available.`, '', 'Download & Restart')
    } else if (manual) {
      setUpdateUI(
        "You're up to date.",
        `Veil App ${await getVersion()} is the latest version.`,
        'Check for Updates',
      )
    }
  } catch (err) {
    console.error('update check failed', err)
    // Startup checks stay quiet; only the manual one reports.
    if (manual) setUpdateUI('Update check failed.', String(err), 'Check for Updates')
  } finally {
    updateButton.disabled = false
  }
}

async function downloadAndInstall(): Promise<void> {
  if (!pendingUpdate) return
  updateButton.disabled = true
  let total = 0
  let got = 0
  try {
    await pendingUpdate.downloadAndInstall((ev) => {
      if (ev.event === 'Started') {
        total = ev.data.contentLength ?? 0
      } else if (ev.event === 'Progress') {
        got += ev.data.chunkLength
        setUpdateUI(
          'Downloading update…',
          total ? `${Math.round((got / total) * 100)}%` : '',
          'Downloading…',
        )
      } else if (ev.event === 'Finished') {
        setUpdateUI('Restarting…', '', 'Downloading…')
      }
    })
    await relaunch()
  } catch (err) {
    console.error('update install failed', err)
    setUpdateUI('Update failed.', String(err), 'Download & Restart')
    updateButton.disabled = false
  }
}

async function initUpdater(): Promise<void> {
  updateButton.addEventListener('click', () => {
    void (pendingUpdate ? downloadAndInstall() : checkForUpdate(true))
  })
  if (import.meta.env.DEV) {
    setUpdateUI("You're up to date.", 'Development build — updates are disabled.', null)
  } else {
    updateButton.hidden = false
    void checkForUpdate(false)
  }
}

window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') void invoke('hide_window', { label: 'settings' })
})

initTabs()
void initLaunchAtLogin()
void initShowMemory()
void initVersion()
void initUpdater()
