import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { relaunch } from '@tauri-apps/plugin-process'
import { check, type Update } from '@tauri-apps/plugin-updater'

import './style.css'

interface AppInfo {
  name: string
  pid: number
  memoryBytes: number
  iconPng: string
  // Whether the app's menu bar icon hides with "Hide icons"; null when it
  // has no icon to choose for.
  veiled: boolean | null
}

// Same shape as the Rust `Settings`.
interface Settings {
  showMemoryUsage: boolean
}

let apps: AppInfo[] = []
let settings: Settings = { showMemoryUsage: true }
let sortBy: 'name' | 'memory' = 'name'

const grid = document.querySelector<HTMLDivElement>('#grid')!
const count = document.querySelector<HTMLSpanElement>('#app-count')!
const hideBtn = document.querySelector<HTMLButtonElement>('#hide-icons')!
const sortNameBtn = document.querySelector<HTMLButtonElement>('#sort-name')!
const sortMemoryBtn = document.querySelector<HTMLButtonElement>('#sort-memory')!

// Existing cards keyed by PID so refreshes update nodes in place. Recreating
// the whole grid every 5s re-decodes icons and re-renders the blurred panel,
// which shows up as a visible flicker.
const cardEls = new Map<number, HTMLElement>()

const EYE_PATH =
  '<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>'
const EYE_ICON = `<svg viewBox="0 0 24 24">${EYE_PATH}</svg>`
const EYE_OFF_ICON = `<svg viewBox="0 0 24 24">${EYE_PATH}<path d="M4 4l16 16"/></svg>`

function buildCard(app: AppInfo): HTMLElement {
  const card = document.createElement('div')
  card.className = 'app-card'

  const iconWrap = document.createElement('div')
  iconWrap.className = 'app-icon'
  if (app.iconPng) {
    const img = document.createElement('img')
    img.src = `data:image/png;base64,${app.iconPng}`
    img.alt = ''
    img.draggable = false
    iconWrap.appendChild(img)
  } else {
    const letter = document.createElement('div')
    letter.className = 'fallback'
    letter.textContent = app.name.charAt(0).toUpperCase()
    iconWrap.appendChild(letter)
  }

  const quit = document.createElement('button')
  quit.className = 'quit'
  quit.textContent = '✕'
  quit.title = 'Quit'
  quit.addEventListener('click', (e) => {
    e.stopPropagation()
    void quitApp(app.pid, e.altKey)
  })
  iconWrap.appendChild(quit)

  const veil = document.createElement('button')
  veil.className = 'veil'
  veil.addEventListener('click', (e) => {
    e.stopPropagation()
    void toggleVeiled(app.pid, veil)
  })
  iconWrap.appendChild(veil)

  const name = document.createElement('div')
  name.className = 'app-name'
  name.textContent = app.name
  name.title = app.name

  const mem = document.createElement('div')
  mem.className = 'app-memory'

  card.append(iconWrap, name, mem)
  card.addEventListener('click', () => void activate(app.pid))
  card.addEventListener('contextmenu', (e) => {
    e.preventDefault()
    void quitApp(app.pid, false)
  })
  return card
}

function render(): void {
  count.textContent = `${apps.length} apps`
  // Re-read on every render: the setting lives in the settings window, and
  // polling alongside the 5s list_apps refresh is the cheapest sync.
  grid.classList.toggle('hide-memory', !settings.showMemoryUsage)
  const list = sorted()
  const activePids = new Set(list.map((a) => a.pid))

  for (const [pid, el] of cardEls) {
    if (!activePids.has(pid)) {
      el.remove()
      cardEls.delete(pid)
    }
  }

  if (list.length === 0) {
    document.getElementById('empty')?.remove()
    const empty = document.createElement('p')
    empty.id = 'empty'
    empty.textContent = 'No menu bar apps detected'
    grid.appendChild(empty)
    return
  }
  document.getElementById('empty')?.remove()

  // Place cards in sorted order, but only move a node when it is not already
  // in its expected slot — an unchanged refresh then touches no DOM at all.
  let prev: Element | null = null
  for (const app of list) {
    let card: HTMLElement | undefined = cardEls.get(app.pid)
    if (!card) {
      card = buildCard(app)
      cardEls.set(app.pid, card)
    }

    const mem = card.querySelector<HTMLDivElement>('.app-memory')!
    const heavy = app.memoryBytes >= 1024 ** 3
    mem.textContent = formatMemory(app.memoryBytes)
    mem.classList.toggle('heavy', heavy)

    const veil = card.querySelector<HTMLButtonElement>('.veil')!
    const state = String(app.veiled)
    // Only touch the DOM when the state changed, like the card order below.
    if (veil.dataset.state !== state) {
      veil.dataset.state = state
      veil.hidden = app.veiled === null
      veil.classList.toggle('pinned', app.veiled === false)
      veil.innerHTML = app.veiled === false ? EYE_ICON : EYE_OFF_ICON
      veil.title =
        app.veiled === false
          ? 'Stays visible · click to hide it with the other icons'
          : 'Hides with the other icons · click to keep it visible'
    }

    const expected: Element | null = prev ? prev.nextElementSibling : grid.firstElementChild
    if (card !== expected) {
      if (prev) {
        prev.after(card)
      } else {
        grid.prepend(card)
      }
    }
    prev = card
  }
}

function formatMemory(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`
  if (bytes > 0) return `${Math.round(bytes / 1024)} KB`
  return ''
}

function sorted(): AppInfo[] {
  const copy = [...apps]
  if (sortBy === 'memory') {
    copy.sort((a, b) => b.memoryBytes - a.memoryBytes)
  } else {
    copy.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))
  }
  return copy
}

async function refresh(): Promise<void> {
  try {
    settings = await invoke<Settings>('get_settings')
  } catch (err) {
    console.error('get_settings failed', err)
  }
  try {
    apps = await invoke<AppInfo[]>('list_apps')
  } catch (err) {
    console.error('list_apps failed', err)
    return
  }
  render()
}

async function quitApp(pid: number, force: boolean): Promise<void> {
  await invoke('quit_app', { pid, force })
  render()
}

// Moves the app's icon across the wall, which takes about a second and
// borrows the mouse, so one move at a time.
let moving = false

async function toggleVeiled(pid: number, button: HTMLButtonElement): Promise<void> {
  const app = apps.find((a) => a.pid === pid)
  if (moving || !app || app.veiled === null) return
  moving = true
  button.disabled = true
  try {
    await invoke('set_app_veiled', { pid, veiled: !app.veiled })
  } catch (err) {
    console.error('set_app_veiled failed', err)
  }
  moving = false
  button.disabled = false
  // Shows where the icon really ended up, moved or not.
  await refresh()
}

async function activate(pid: number): Promise<void> {
  await invoke('activate_app', { pid })
}

function setSort(mode: 'name' | 'memory'): void {
  sortBy = mode
  sortNameBtn.classList.toggle('active', mode === 'name')
  sortMemoryBtn.classList.toggle('active', mode === 'memory')
  render()
}

sortNameBtn.addEventListener('click', () => setSort('name'))
sortMemoryBtn.addEventListener('click', () => setSort('memory'))

function setIconsHidden(hidden: boolean): void {
  hideBtn.textContent = hidden ? 'Show icons' : 'Hide icons'
}

hideBtn.addEventListener('click', async () => {
  setIconsHidden(await invoke<boolean>('toggle_menu_bar_icons'))
})

// Launched at login, the icons start out hidden.
void invoke<boolean>('icons_hidden').then(setIconsHidden)

// Opening an app through its menu bar icon brings the icons back.
void listen<boolean>('icons-hidden', (e) => setIconsHidden(e.payload))

// Update banner: the panel is the only window users open day to day, so an
// available update is announced here rather than only in Settings → About.
const updateBar = document.querySelector<HTMLDivElement>('#update')!
const updateText = document.querySelector<HTMLSpanElement>('#update-text')!
const updateBtn = document.querySelector<HTMLButtonElement>('#update-install')!

let pendingUpdate: Update | null = null

async function checkForUpdate(): Promise<void> {
  if (pendingUpdate) return
  try {
    pendingUpdate = await check()
  } catch (err) {
    console.error('update check failed', err)
    return
  }
  if (!pendingUpdate) return
  updateText.textContent = `Veil App ${pendingUpdate.version} is available`
  updateBar.hidden = false
}

updateBtn.addEventListener('click', async () => {
  if (!pendingUpdate) return
  updateBtn.disabled = true
  updateBtn.textContent = 'Updating…'
  try {
    await pendingUpdate.downloadAndInstall()
    await relaunch()
  } catch (err) {
    console.error('update install failed', err)
    updateBtn.textContent = 'Retry'
    updateBtn.disabled = false
  }
})

// Dev builds have no matching release. The app stays running for weeks, so
// re-check periodically instead of only at launch.
if (!import.meta.env.DEV) {
  void checkForUpdate()
  setInterval(() => void checkForUpdate(), 6 * 60 * 60 * 1000)
}

window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') void invoke('hide_window', { label: 'panel' })
})

void refresh()
setInterval(() => void refresh(), 5000)
