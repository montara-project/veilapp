import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
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

window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') void invoke('hide_window', { label: 'settings' })
})

initTabs()
void initLaunchAtLogin()
void initShowMemory()
void initVersion()
