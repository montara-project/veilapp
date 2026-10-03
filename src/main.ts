import { invoke } from '@tauri-apps/api/core'
import './style.css'

interface AppInfo {
  name: string
  pid: number
  memoryBytes: number
  iconPng: string
}

// Same shape as the Rust `Settings`.
interface Settings {
  showMemoryUsage: boolean
}

const isTauri = '__TAURI_INTERNALS__' in window

// Same shape as the Rust `AppInfo`; used when previewing the panel in a
// regular browser where the Tauri IPC is not available.
const MOCK_APPS: AppInfo[] = [
  { name: 'ChatGPT', pid: 101, memoryBytes: 4_620_000_000, iconPng: '' },
  { name: 'Claude', pid: 102, memoryBytes: 3_590_000_000, iconPng: '' },
  { name: 'Slack', pid: 103, memoryBytes: 1_375_000_000, iconPng: '' },
  { name: 'Ollama', pid: 104, memoryBytes: 860_000_000, iconPng: '' },
  { name: 'OneDrive', pid: 105, memoryBytes: 247_500_000, iconPng: '' },
  { name: 'Mole', pid: 106, memoryBytes: 139_500_000, iconPng: '' },
  { name: 'Dropover', pid: 107, memoryBytes: 80_700_000, iconPng: '' },
  { name: 'CleanMyMac', pid: 108, memoryBytes: 51_400_000, iconPng: '' },
  { name: 'Itsycal', pid: 109, memoryBytes: 35_700_000, iconPng: '' },
]

let apps: AppInfo[] = []
let settings: Settings = { showMemoryUsage: true }
let sortBy: 'name' | 'memory' = 'name'
let iconsHidden = false

const grid = document.querySelector<HTMLDivElement>('#grid')!
const count = document.querySelector<HTMLSpanElement>('#app-count')!
const hideBtn = document.querySelector<HTMLButtonElement>('#hide-icons')!
const sortNameBtn = document.querySelector<HTMLButtonElement>('#sort-name')!
const sortMemoryBtn = document.querySelector<HTMLButtonElement>('#sort-memory')!

// Existing cards keyed by PID so refreshes update nodes in place. Recreating
// the whole grid every 5s re-decodes icons and re-renders the blurred panel,
// which shows up as a visible flicker.
const cardEls = new Map<number, HTMLElement>()

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

    const expected: Element | null = prev
      ? prev.nextElementSibling
      : grid.firstElementChild
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
    copy.sort((a, b) =>
      a.name.toLowerCase().localeCompare(b.name.toLowerCase())
    )
  }
  return copy
}

async function refresh(): Promise<void> {
  if (isTauri) {
    try {
      settings = await invoke<Settings>('get_settings')
    } catch (err) {
      console.error('get_settings failed', err)
    }
  }
  try {
    apps = isTauri ? await invoke<AppInfo[]>('list_apps') : MOCK_APPS
  } catch (err) {
    console.error('list_apps failed', err)
    return
  }
  render()
}

async function quitApp(pid: number, force: boolean): Promise<void> {
  if (isTauri) {
    await invoke('quit_app', { pid, force })
  } else {
    apps = apps.filter((a) => a.pid !== pid)
  }
  render()
}

async function activate(pid: number): Promise<void> {
  if (isTauri) await invoke('activate_app', { pid })
}

function setSort(mode: 'name' | 'memory'): void {
  sortBy = mode
  sortNameBtn.classList.toggle('active', mode === 'name')
  sortMemoryBtn.classList.toggle('active', mode === 'memory')
  render()
}

sortNameBtn.addEventListener('click', () => setSort('name'))
sortMemoryBtn.addEventListener('click', () => setSort('memory'))

hideBtn.addEventListener('click', async () => {
  iconsHidden = isTauri
    ? await invoke<boolean>('toggle_menu_bar_icons')
    : !iconsHidden
  hideBtn.textContent = iconsHidden ? 'Show icons' : 'Hide icons'
})

window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && isTauri) void invoke('hide_panel')
})

void refresh()
setInterval(() => void refresh(), 5000)
