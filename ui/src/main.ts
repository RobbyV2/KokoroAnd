import '@material/web/button/filled-button.js'
import '@material/web/button/filled-tonal-button.js'
import '@material/web/button/outlined-button.js'
import '@material/web/button/text-button.js'
import '@material/web/dialog/dialog.js'
import '@material/web/progress/linear-progress.js'
import '@material/web/select/outlined-select.js'
import '@material/web/select/select-option.js'
import '@material/web/slider/slider.js'
import '@material/web/switch/switch.js'
import '@material/web/textfield/outlined-text-field.js'
import type { MdDialog } from '@material/web/dialog/dialog.js'
import type { MdLinearProgress } from '@material/web/progress/linear-progress.js'
import type { MdOutlinedSelect } from '@material/web/select/outlined-select.js'
import type { MdSlider } from '@material/web/slider/slider.js'
import type { MdSwitch } from '@material/web/switch/switch.js'
import type { MdOutlinedTextField } from '@material/web/textfield/outlined-text-field.js'
import { getVersion } from '@tauri-apps/api/app'
import './style.css'
import { backend, defaultSettings, type CustomDict, type Variant } from './backend'

const voiceGroups: [string, boolean, string[]][] = [
  [
    'American English',
    false,
    [
      'af_alloy',
      'af_aoede',
      'af_bella',
      'af_heart',
      'af_jessica',
      'af_kore',
      'af_nicole',
      'af_nova',
      'af_river',
      'af_sarah',
      'af_sky',
      'am_adam',
      'am_echo',
      'am_eric',
      'am_fenrir',
      'am_liam',
      'am_michael',
      'am_onyx',
      'am_puck',
      'am_santa',
    ],
  ],
  [
    'British English',
    false,
    [
      'bf_alice',
      'bf_emma',
      'bf_isabella',
      'bf_lily',
      'bm_daniel',
      'bm_fable',
      'bm_george',
      'bm_lewis',
    ],
  ],
  ['Japanese', false, ['jf_alpha', 'jf_gongitsune', 'jf_nezumi', 'jf_tebukuro', 'jm_kumo']],
  [
    'Mandarin Chinese',
    false,
    [
      'zf_xiaobei',
      'zf_xiaoni',
      'zf_xiaoxiao',
      'zf_xiaoyi',
      'zm_yunjian',
      'zm_yunxi',
      'zm_yunxia',
      'zm_yunyang',
    ],
  ],
  ['Spanish', true, ['ef_dora', 'em_alex', 'em_santa']],
  ['French', true, ['ff_siwis']],
  ['Hindi', true, ['hf_alpha', 'hf_beta', 'hm_omega', 'hm_psi']],
  ['Italian', true, ['if_sara', 'im_nicola']],
  ['Brazilian Portuguese', true, ['pf_dora', 'pm_alex', 'pm_santa']],
]

const $ = <T extends HTMLElement>(sel: string): T => document.querySelector<T>(sel)!

type View = 'onboarding' | 'home' | 'settings'
const views: Record<View, HTMLElement> = {
  onboarding: $('#view-onboarding'),
  home: $('#view-home'),
  settings: $('#view-settings'),
}
const tabs = document.querySelectorAll<HTMLButtonElement>('nav button')

const show = (view: View) => {
  for (const [name, el] of Object.entries(views)) el.hidden = name !== view
  for (const tab of tabs) tab.classList.toggle('active', tab.dataset.view === view)
  document.body.classList.toggle('onboarding', view === 'onboarding')
  if (view === 'home') void refreshHome()
  if (view === 'settings') {
    void refreshApiStatus()
    void refreshModel()
  }
}
for (const tab of tabs) tab.addEventListener('click', () => show(tab.dataset.view as View))

const fmtBytes = (n: number) =>
  n >= 1 << 30 ? `${(n / (1 << 30)).toFixed(2)} GB` : `${(n / (1 << 20)).toFixed(1)} MB`
const errMsg = (e: unknown) =>
  typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e)

let snackbarTimer = 0
const snackbar = (text: string, action?: string, onAction?: () => void) => {
  const bar = $('#snackbar')
  const btn = $('#snackbar-action')
  $('#snackbar-text').textContent = text
  btn.hidden = !action
  btn.textContent = action ?? ''
  btn.onclick = () => {
    bar.hidden = true
    onAction?.()
  }
  bar.hidden = false
  clearTimeout(snackbarTimer)
  snackbarTimer = window.setTimeout(() => (bar.hidden = true), action ? 8000 : 4000)
}

let settings = defaultSettings()
const saveSettings = () => backend.settingsSet(settings).catch(e => snackbar(errMsg(e)))

const onboarding = () => {
  const bar = $<MdLinearProgress>('#ob-bar')
  const status = $('#ob-status')
  const download = $('#ob-download')
  const cancel = $('#ob-cancel')
  const enter = $('#ob-enter')

  const start = () => {
    bar.hidden = false
    bar.indeterminate = true
    cancel.hidden = false
    download.hidden = true
    status.textContent = 'Starting download'
    const picked =
      document.querySelector<HTMLInputElement>('input[name="ob-variant"]:checked')?.value ?? 'fp16'
    backend.downloadStart(picked as Variant).catch(e => fail(errMsg(e)))
  }
  const fail = (msg: string) => {
    bar.hidden = true
    cancel.hidden = true
    download.hidden = false
    download.textContent = 'Resume download'
    status.textContent = msg
  }
  download.addEventListener('click', start)
  cancel.addEventListener('click', () => {
    backend.downloadCancel().catch(e => snackbar(errMsg(e)))
    fail('Download paused')
  })
  enter.addEventListener('click', () => {
    localStorage.setItem('onboarded', '1')
    show('home')
  })
  void backend.onDownloadProgress(({ file, received, total }) => {
    bar.hidden = false
    cancel.hidden = false
    download.hidden = true
    bar.indeterminate = total === null
    if (total !== null) bar.value = received / total
    status.textContent = `${file} ${fmtBytes(received)}${total !== null ? ` / ${fmtBytes(total)}` : ''}`
  })
  void backend.onDownloadDone(() => {
    bar.hidden = true
    cancel.hidden = true
    download.hidden = true
    enter.hidden = false
    status.textContent = 'Models installed'
    void refreshHome()
  })
  void backend.onDownloadError(msg => fail(msg))
}

const refreshHome = async () => {
  const missing = $('#home-missing')
  const ready = $('#home-ready')
  const st = await backend.installStatus().catch(() => null)
  missing.hidden = st?.installed === true
  ready.hidden = st?.installed !== true
  if (st?.installed === true)
    $('#home-status').textContent =
      `${st.voice_count} voices installed, ${fmtBytes(st.model_bytes)}`
}

const demo = () => {
  const play = $<HTMLElement & { disabled: boolean }>('#demo-play')
  const state = $('#demo-state')
  const error = $('#demo-error')
  void backend.onSynthPhase(phase => (state.textContent = phase))
  play.addEventListener('click', async () => {
    play.disabled = true
    error.hidden = true
    state.textContent = 'phonemizing'
    try {
      const text = $<MdOutlinedTextField>('#demo-text').value
      const wav = await backend.demoSynth(text, settings.voice, settings.speed)
      const url = URL.createObjectURL(new Blob([wav.buffer as ArrayBuffer], { type: 'audio/wav' }))
      const audio = new Audio(url)
      audio.onended = () => {
        state.textContent = 'idle'
        play.disabled = false
        URL.revokeObjectURL(url)
      }
      state.textContent = 'playing'
      await audio.play()
    } catch (e) {
      error.textContent = errMsg(e)
      error.hidden = false
      state.textContent = 'idle'
      play.disabled = false
    }
  })
  $('#home-reinstall').addEventListener('click', () => show('onboarding'))
}

const refreshApiStatus = async () => {
  const st = await backend.serverStatus().catch(() => null)
  const copy = $('#api-copy')
  const { host, port } = settings.server
  const url = `http://${host}:${port}/v1`
  $('#api-status').textContent = st?.running === true ? `Running at ${st.addr ?? url}` : 'Stopped'
  copy.hidden = st?.running !== true
  copy.onclick = () =>
    navigator.clipboard.writeText(url).then(
      () => snackbar('Copied'),
      () => snackbar(url)
    )
}

let activeVariant: Variant | null = null
let switching = false

const refreshModel = async () => {
  const st = await backend.installStatus().catch(() => null)
  activeVariant = st?.variant ?? null
  const other: Variant = activeVariant === 'fp16' ? 'fp32' : 'fp16'
  $('#model-active').textContent =
    activeVariant === null ? 'No model installed' : `Active model: ${activeVariant}`
  const btn = $<HTMLElement & { disabled: boolean }>('#model-switch')
  btn.textContent = `Switch to ${other}`
  btn.disabled = activeVariant === null || switching
}

const modelSection = () => {
  const btn = $<HTMLElement & { disabled: boolean }>('#model-switch')
  btn.addEventListener('click', () => {
    if (activeVariant === null || switching) return
    switching = true
    btn.disabled = true
    const target: Variant = activeVariant === 'fp16' ? 'fp32' : 'fp16'
    backend.downloadStart(target).catch(e => {
      switching = false
      snackbar(errMsg(e))
      void refreshModel()
    })
  })
  void backend.onDownloadProgress(({ file, received, total }) => {
    if (switching)
      $('#model-active').textContent =
        `${file} ${fmtBytes(received)}${total !== null ? ` / ${fmtBytes(total)}` : ''}`
  })
  void backend.onDownloadDone(async () => {
    if (!switching) return
    switching = false
    try {
      const st = await backend.serverStatus().catch(() => null)
      const wasRunning = st?.running === true
      if (wasRunning) await backend.serverStop()
      await backend.engineInit(settings.threads)
      if (wasRunning) await backend.serverStart()
      snackbar('Model switched')
    } catch (e) {
      snackbar(errMsg(e))
    }
    await refreshModel()
  })
  void backend.onDownloadError(msg => {
    if (!switching) return
    switching = false
    snackbar(msg)
    void refreshModel()
  })
}

let dict: CustomDict = {}
const renderDict = () => {
  const box = $('#dict-entries')
  box.replaceChildren(
    ...Object.entries(dict).map(([word, ipa]) => {
      const row = document.createElement('div')
      row.className = 'dict-row'
      const label = document.createElement('span')
      label.textContent = `${word} → ${ipa}`
      const del = document.createElement('md-text-button')
      del.textContent = 'Remove'
      del.addEventListener('click', () => {
        delete dict[word]
        void pushDict()
      })
      row.append(label, del)
      return row
    })
  )
}
const pushDict = async () => {
  renderDict()
  await backend.dictImport(dict).catch(e => snackbar(errMsg(e)))
}

const dictionary = () => {
  $('#dict-add').addEventListener('click', () => {
    const word = $<MdOutlinedTextField>('#dict-word')
    const ipa = $<MdOutlinedTextField>('#dict-ipa')
    if (!word.value || !ipa.value) return
    dict[word.value.toLowerCase()] = ipa.value
    word.value = ''
    ipa.value = ''
    void pushDict()
  })
  $('#dict-import').addEventListener('click', () => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = 'application/json'
    input.onchange = async () => {
      const file = input.files?.[0]
      if (!file) return
      try {
        const parsed: unknown = JSON.parse(await file.text())
        if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed))
          throw new Error('expected a JSON object of word to IPA')
        dict = Object.fromEntries(Object.entries(parsed).map(([k, v]) => [k, String(v)]))
        await pushDict()
        snackbar(`Imported ${Object.keys(dict).length} entries`)
      } catch (e) {
        snackbar(errMsg(e))
      }
    }
    input.click()
  })
  $('#dict-export').addEventListener('click', async () => {
    try {
      dict = await backend.dictExport()
      renderDict()
      const url = URL.createObjectURL(
        new Blob([JSON.stringify(dict, null, 2)], { type: 'application/json' })
      )
      const a = document.createElement('a')
      a.href = url
      a.download = 'kokoro-dictionary.json'
      a.click()
      URL.revokeObjectURL(url)
    } catch (e) {
      snackbar(errMsg(e))
    }
  })
}

const settingsScreen = () => {
  const voice = $<MdOutlinedSelect>('#set-voice')
  for (const [group, experimental, names] of voiceGroups) {
    const header = document.createElement('md-select-option')
    header.setAttribute('disabled', '')
    header.innerHTML = `<div slot="headline">${group}${experimental ? ' (experimental)' : ''}</div>`
    voice.append(header)
    for (const name of names) {
      const opt = document.createElement('md-select-option')
      opt.setAttribute('value', name)
      opt.innerHTML = `<div slot="headline">${name}</div>`
      voice.append(opt)
    }
  }

  const mix = $<MdOutlinedTextField>('#set-mix')
  const speed = $<MdSlider>('#set-speed')
  const speedVal = $('#set-speed-val')
  const threads = $<MdOutlinedTextField>('#set-threads')
  const server = $<MdSwitch>('#set-server')
  const host = $<MdOutlinedSelect>('#set-host')
  const port = $<MdOutlinedTextField>('#set-port')
  const always = $<MdSwitch>('#set-always')

  const apply = () => {
    const mixed = settings.voice.includes('+') || settings.voice.includes('(')
    voice.value = mixed ? 'af_bella' : settings.voice
    mix.value = mixed ? settings.voice : ''
    speed.value = settings.speed
    speedVal.textContent = `${settings.speed.toFixed(2)}x`
    threads.value = String(settings.threads.intra ?? 0)
    server.selected = settings.server_enabled
    always.selected = settings.always_on
    host.value = settings.server.host
    port.value = String(settings.server.port)
    $<MdSwitch>('#set-emotion').selected = settings.emotion_tags
    $<MdSwitch>('#set-punct').selected = settings.smart_punct
  }
  apply()

  const currentVoice = () =>
    mix.value.trim() !== '' ? mix.value.trim() : voice.value || 'af_bella'
  voice.addEventListener('change', () => {
    settings.voice = currentVoice()
    void saveSettings()
  })
  mix.addEventListener('change', () => {
    settings.voice = currentVoice()
    void saveSettings()
  })
  speed.addEventListener('change', () => {
    settings.speed = speed.value ?? 1
    speedVal.textContent = `${settings.speed.toFixed(2)}x`
    void saveSettings()
  })
  threads.addEventListener('change', () => {
    const n = Number(threads.value)
    settings.threads.intra = Number.isFinite(n) && n > 0 ? Math.floor(n) : null
    void saveSettings()
  })
  server.addEventListener('change', async () => {
    settings.server_enabled = server.selected
    await saveSettings()
    try {
      await (server.selected ? backend.serverStart() : backend.serverStop())
    } catch (e) {
      snackbar(errMsg(e))
      server.selected = !server.selected
      settings.server_enabled = server.selected
      await saveSettings()
    }
    await refreshApiStatus()
  })
  always.addEventListener('change', async () => {
    settings.always_on = always.selected
    try {
      await backend.settingsSet(settings)
    } catch (e) {
      snackbar(errMsg(e))
      always.selected = !always.selected
      settings.always_on = always.selected
      await backend.settingsSet(settings).catch(() => undefined)
    }
  })
  host.addEventListener('change', () => {
    settings.server.host = host.value
    void saveSettings().then(refreshApiStatus)
  })
  port.addEventListener('change', () => {
    const n = Number(port.value)
    settings.server.port = Number.isInteger(n) && n > 0 && n < 65536 ? n : 8471
    port.value = String(settings.server.port)
    void saveSettings().then(refreshApiStatus)
  })
  $<MdSwitch>('#set-emotion').addEventListener('change', e => {
    settings.emotion_tags = (e.target as MdSwitch).selected
    void saveSettings()
  })
  $<MdSwitch>('#set-punct').addEventListener('change', e => {
    settings.smart_punct = (e.target as MdSwitch).selected
    void saveSettings()
  })

  $('#set-clear-cache').addEventListener('click', async () => {
    try {
      snackbar(`Freed ${fmtBytes(await backend.clearCache())}`)
    } catch (e) {
      snackbar(errMsg(e))
    }
  })

  const dialog = $<MdDialog>('#confirm-delete')
  $('#set-delete').addEventListener('click', () => void dialog.show())
  dialog.addEventListener('closed', async () => {
    if (dialog.returnValue !== 'delete') return
    try {
      await backend.deleteModels()
      await refreshHome()
      snackbar('Models deleted', 'Reinstall', () => show('onboarding'))
    } catch (e) {
      snackbar(errMsg(e))
    }
  })

  $('#set-battery').addEventListener('click', () =>
    backend.batteryExemption().catch(e => snackbar(errMsg(e)))
  )

  void getVersion().then(
    v => ($('#about-version').textContent = `KokoroAnd ${v}`),
    () => undefined
  )
}

const applyInsets = () => {
  const insets = (window as unknown as { AndroidInsets?: { top(): number; bottom(): number } })
    .AndroidInsets
  if (insets === undefined) return
  const root = document.documentElement.style
  root.setProperty('--inset-top', `${insets.top()}px`)
  root.setProperty('--inset-bottom', `${insets.bottom()}px`)
}

const init = async () => {
  applyInsets()
  window.addEventListener('resize', applyInsets)
  void backend.onAlwaysOnError(msg => snackbar(msg))
  onboarding()
  demo()
  dictionary()
  modelSection()
  settings = await backend.settingsGet().catch(() => defaultSettings())
  settingsScreen()
  dict = await backend.dictExport().catch(() => ({}))
  renderDict()
  const st = await backend.installStatus().catch(() => null)
  const firstRun = st?.installed !== true && localStorage.getItem('onboarded') === null
  show(firstRun ? 'onboarding' : 'home')
}

void init()
