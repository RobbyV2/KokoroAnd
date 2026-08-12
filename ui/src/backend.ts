import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface ThreadConfig {
  inter: number
  intra: number | null
}

export interface ServerConfig {
  host: string
  port: number
}

export interface Settings {
  voice: string
  speed: number
  threads: ThreadConfig
  server: ServerConfig
  server_enabled: boolean
  always_on: boolean
  emotion_tags: boolean
  smart_punct: boolean
}

export interface DownloadProgress {
  file: string
  received: number
  total: number | null
}

export type Variant = 'fp32' | 'fp16'

export interface InstallStatus {
  installed: boolean
  variant: Variant | null
  model_bytes: number
  voice_count: number
}

export interface ServerStatus {
  running: boolean
  addr: string | null
}

export type CustomDict = Record<string, string>

export type SynthPhase = 'phonemizing' | 'synthesizing'

export const defaultSettings = (): Settings => ({
  voice: 'af_bella',
  speed: 1,
  threads: { inter: 1, intra: null },
  server: { host: '127.0.0.1', port: 8471 },
  server_enabled: false,
  always_on: false,
  emotion_tags: false,
  smart_punct: false,
})

export const backend = {
  downloadStart: (variant: Variant) => invoke<void>('download_start', { variant }),
  downloadCancel: () => invoke<void>('download_cancel'),
  installStatus: () => invoke<InstallStatus>('install_status'),
  engineInit: (threads: ThreadConfig) => invoke<void>('engine_init', { threads }),
  demoSynth: async (text: string, voice: string, speed: number) =>
    new Uint8Array(await invoke<number[]>('demo_synth', { text, voice, speed })),
  serverStart: () => invoke<ServerStatus>('server_start'),
  serverStop: () => invoke<ServerStatus>('server_stop'),
  serverStatus: () => invoke<ServerStatus>('server_status'),
  settingsGet: () => invoke<Settings>('settings_get'),
  settingsSet: (settings: Settings) => invoke<void>('settings_set', { settings }),
  dictImport: (dict: CustomDict) => invoke<number>('dict_import', { dict }),
  dictExport: () => invoke<CustomDict>('dict_export'),
  clearCache: () => invoke<number>('clear_cache'),
  deleteModels: () => invoke<void>('delete_models'),
  batteryExemption: () => invoke<void>('battery_exemption'),
  onDownloadProgress: (fn: (p: DownloadProgress) => void): Promise<UnlistenFn> =>
    listen<DownloadProgress>('download-progress', e => fn(e.payload)),
  onDownloadDone: (fn: () => void): Promise<UnlistenFn> =>
    listen<null>('download-done', () => fn()),
  onDownloadError: (fn: (msg: string) => void): Promise<UnlistenFn> =>
    listen<string>('download-error', e => fn(e.payload)),
  onSynthPhase: (fn: (p: SynthPhase) => void): Promise<UnlistenFn> =>
    listen<SynthPhase>('synth-phase', e => fn(e.payload)),
  onAlwaysOnError: (fn: (msg: string) => void): Promise<UnlistenFn> =>
    listen<string>('always-on-error', e => fn(e.payload)),
}
