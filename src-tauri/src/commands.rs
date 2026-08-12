use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use futures_util::StreamExt;
use kokoro_core::dsp::DspFlags;
use kokoro_core::{
    CancelToken, Chunker, Engine, EngineConfig, Pcm, SAMPLE_RATE, ThreadConfig, VoiceId, VoiceSpec,
};
use kokoro_g2p::{CustomDict, G2p, Lang};
use kokoro_server::{Server, ServerConfig, SynthEngine};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Serialize)]
pub struct CmdError(String);

impl From<anyhow::Error> for CmdError {
    fn from(err: anyhow::Error) -> Self {
        Self(format!("{err:#}"))
    }
}

impl From<std::io::Error> for CmdError {
    fn from(err: std::io::Error) -> Self {
        Self(err.to_string())
    }
}

impl From<serde_json::Error> for CmdError {
    fn from(err: serde_json::Error) -> Self {
        Self(err.to_string())
    }
}

impl From<kokoro_g2p::Error> for CmdError {
    fn from(err: kokoro_g2p::Error) -> Self {
        Self(err.to_string())
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub file: String,
    pub received: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    Fp32,
    Fp16,
}

impl Variant {
    fn spec(self) -> &'static FileSpec {
        match self {
            Self::Fp32 => &MODELS[0],
            Self::Fp16 => &MODELS[1],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallStatus {
    pub installed: bool,
    pub variant: Option<Variant>,
    pub model_bytes: u64,
    pub voice_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub running: bool,
    pub addr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub voice: String,
    pub speed: f32,
    pub threads: ThreadConfig,
    pub server: ServerConfig,
    pub server_enabled: bool,
    #[serde(default)]
    pub always_on: bool,
    #[serde(flatten)]
    pub dsp: DspFlags,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            voice: "af_bella".into(),
            speed: 1.0,
            threads: ThreadConfig::default(),
            server: ServerConfig::default(),
            server_enabled: false,
            always_on: false,
            dsp: DspFlags::default(),
        }
    }
}

#[derive(Debug, Serialize)]
struct FileSpec {
    name: &'static str,
    #[serde(skip)]
    url: &'static str,
    size: u64,
    sha256: &'static str,
}

const MODELS: [FileSpec; 2] = [
    FileSpec {
        name: "kokoro-v1.0.onnx",
        url: "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx",
        size: 325_532_387,
        sha256: "7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5",
    },
    FileSpec {
        name: "kokoro-v1.0.fp16.onnx",
        url: "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.fp16.onnx",
        size: 177_464_787,
        sha256: "c1610a859f3bdea01107e73e50100685af38fff88f5cd8e5c56df109ec880204",
    },
];

const VOICES: FileSpec = FileSpec {
    name: "voices-v1.0.bin",
    url: "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin",
    size: 28_214_398,
    sha256: "bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d",
};

fn active_variant(models_dir: &Path) -> Option<Variant> {
    let text = std::fs::read_to_string(models_dir.join("manifest.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    serde_json::from_value(value.get("variant")?.clone()).ok()
}

const VOICE_COUNT: usize = 54;

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn read_settings(config: &Path) -> Settings {
    std::fs::read_to_string(config.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn read_dict(config: &Path) -> CustomDict {
    std::fs::read_to_string(config.join("dict.json"))
        .ok()
        .and_then(|s| CustomDict::from_json(&s).ok())
        .unwrap_or_default()
}

pub struct Tts {
    pub engine: Engine,
    pub g2p: G2p,
    pub dict: Arc<Mutex<CustomDict>>,
    pub flags: Arc<Mutex<DspFlags>>,
}

impl Tts {
    pub fn load(
        models_dir: &Path,
        threads: ThreadConfig,
        dict: Arc<Mutex<CustomDict>>,
        flags: Arc<Mutex<DspFlags>>,
    ) -> anyhow::Result<Self> {
        let variant = active_variant(models_dir).unwrap_or(Variant::Fp32);
        Ok(Self {
            engine: Engine::new(EngineConfig {
                model_path: models_dir.join(variant.spec().name),
                voices_path: models_dir.join(VOICES.name),
                threads,
            })?,
            g2p: G2p::new()?,
            dict,
            flags,
        })
    }

    pub fn phonemize(&self, text: &str, lang: Lang) -> anyhow::Result<String> {
        let dict = lock(&self.dict).clone();
        Ok(self.g2p.phonemize(text, lang, Some(&dict))?)
    }

    fn lang_of(voice: &VoiceSpec) -> Lang {
        let name = match voice {
            VoiceSpec::Single(VoiceId(name)) => Some(name),
            VoiceSpec::Mix(parts) => parts.first().map(|(VoiceId(name), _)| name),
        };
        name.map_or(Lang::EnUs, |n| Lang::for_voice(n))
    }

    pub fn synthesize_dsp(
        &self,
        text: &str,
        voice: &VoiceSpec,
        speed: f32,
        chunker: &Chunker,
        cancel: &CancelToken,
        on_chunk: &mut dyn FnMut(Pcm) -> kokoro_core::Result<()>,
    ) -> kokoro_core::Result<()> {
        let flags = *lock(&self.flags);
        let lang = Self::lang_of(voice);
        let phonemize = |t: &str| {
            self.phonemize(t, lang)
                .map_err(|e| kokoro_core::Error::Input(format!("{e:#}")))
        };
        self.engine.synthesize_pipeline(
            text, voice, speed, flags, chunker, cancel, &phonemize, on_chunk,
        )
    }
}

impl SynthEngine for Tts {
    fn voices(&self) -> Vec<VoiceId> {
        self.engine.voices()
    }

    fn synthesize_streaming(
        &self,
        text: &str,
        voice: &VoiceSpec,
        speed: f32,
        chunker: &Chunker,
        cancel: &CancelToken,
        on_chunk: &mut dyn FnMut(Pcm) -> kokoro_core::Result<()>,
    ) -> kokoro_core::Result<()> {
        self.synthesize_dsp(text, voice, speed, chunker, cancel, on_chunk)
    }
}

pub struct AppState {
    models_dir: PathBuf,
    config_dir: PathBuf,
    server: tokio::sync::Mutex<Option<Server>>,
    settings: Mutex<Settings>,
    dict: Arc<Mutex<CustomDict>>,
    flags: Arc<Mutex<DspFlags>>,
    download: Mutex<Option<CancelToken>>,
    progress: Arc<Mutex<Option<DownloadProgress>>>,
}

impl AppState {
    pub fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let models_dir = app.path().app_data_dir()?.join("models");
        let config_dir = app.path().app_config_dir()?;
        std::fs::create_dir_all(&models_dir)?;
        std::fs::create_dir_all(&config_dir)?;
        let settings = read_settings(&config_dir);
        let (dict, flags) = match crate::android::TTS.peek() {
            Some(tts) => (tts.dict.clone(), tts.flags.clone()),
            None => (
                Arc::new(Mutex::new(read_dict(&config_dir))),
                Arc::new(Mutex::new(settings.dsp)),
            ),
        };
        Ok(Self {
            models_dir,
            config_dir,
            server: tokio::sync::Mutex::new(None),
            settings: Mutex::new(settings),
            dict,
            flags,
            download: Mutex::new(None),
            progress: Arc::new(Mutex::new(None)),
        })
    }
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).map(|m| m.len()).ok()
}

fn install_status_inner(models_dir: &Path) -> InstallStatus {
    let variant = active_variant(models_dir);
    let complete = |spec: &FileSpec| file_size(&models_dir.join(spec.name)) == Some(spec.size);
    let installed = match variant {
        Some(v) => complete(v.spec()) && complete(&VOICES),
        None => false,
    };
    InstallStatus {
        installed,
        variant,
        model_bytes: MODELS
            .iter()
            .chain([&VOICES])
            .filter_map(|f| file_size(&models_dir.join(f.name)))
            .sum(),
        voice_count: match installed {
            true => VOICE_COUNT,
            false => 0,
        },
    }
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn link_local(spec: &FileSpec, dest: &Path) -> bool {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../temp/models")
        .join(spec.name);
    match file_size(&src) == Some(spec.size) {
        true => {
            let _ = std::fs::remove_file(dest);
            std::os::unix::fs::symlink(&src, dest).is_ok()
        }
        false => false,
    }
}

async fn fetch(
    client: &reqwest::Client,
    spec: &FileSpec,
    dest: &Path,
    cancel: &CancelToken,
    report: &(dyn Fn(u64) + Sync),
) -> anyhow::Result<()> {
    use std::io::Write;
    let part = dest.with_file_name(format!("{}.part", spec.name));
    let mut received = file_size(&part).unwrap_or(0).min(spec.size);
    let resp = client
        .get(spec.url)
        .header(reqwest::header::RANGE, format!("bytes={received}-"))
        .send()
        .await?
        .error_for_status()?;
    let mut file = match received > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        true => std::fs::OpenOptions::new().append(true).open(&part)?,
        false => {
            received = 0;
            std::fs::File::create(&part)?
        }
    };
    let mut stream = resp.bytes_stream();
    let mut last = std::time::Instant::now();
    while let Some(chunk) = stream.next().await {
        anyhow::ensure!(!cancel.is_cancelled(), "download cancelled");
        let chunk = chunk?;
        file.write_all(&chunk)?;
        received += chunk.len() as u64;
        if last.elapsed().as_millis() >= 150 {
            report(received);
            last = std::time::Instant::now();
        }
    }
    file.flush()?;
    drop(file);
    anyhow::ensure!(
        received == spec.size,
        "{}: got {received} bytes, expected {}",
        spec.name,
        spec.size
    );
    let (part2, expected) = (part.clone(), spec.sha256);
    let hash = tauri::async_runtime::spawn_blocking(move || sha256_file(&part2)).await??;
    anyhow::ensure!(hash == expected, "{}: sha256 mismatch", spec.name);
    std::fs::rename(&part, dest)?;
    Ok(())
}

async fn run_download(
    app: AppHandle,
    models_dir: PathBuf,
    variant: Variant,
    cancel: CancelToken,
    progress: Arc<Mutex<Option<DownloadProgress>>>,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .http1_only()
        .build()?;
    for spec in [variant.spec(), &VOICES] {
        let dest = models_dir.join(spec.name);
        if file_size(&dest) == Some(spec.size) {
            continue;
        }
        let report = |received: u64| {
            let p = DownloadProgress {
                file: spec.name.into(),
                received,
                total: Some(spec.size),
            };
            *lock(&progress) = Some(p.clone());
            crate::android::download_notify(spec.name, received, spec.size);
            let _ = app.emit("download-progress", &p);
        };
        if !link_local(spec, &dest) {
            fetch(&client, spec, &dest, &cancel, &report).await?;
        }
        report(spec.size);
    }
    let manifest = serde_json::json!({
        "version": "model-files-v1.0",
        "variant": variant,
        "files": [variant.spec(), &VOICES],
    });
    std::fs::write(
        models_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

async fn load_tts(state: &AppState, threads: Option<ThreadConfig>) -> anyhow::Result<Arc<Tts>> {
    anyhow::ensure!(
        install_status_inner(&state.models_dir).installed,
        "models not installed"
    );
    let threads = threads.unwrap_or_else(|| lock(&state.settings).threads.clone());
    let models_dir = state.models_dir.clone();
    let (dict, flags) = (state.dict.clone(), state.flags.clone());
    let tts =
        tauri::async_runtime::spawn_blocking(move || Tts::load(&models_dir, threads, dict, flags))
            .await??;
    Ok(Arc::new(tts))
}

pub(crate) fn warm(tts: &Arc<Tts>) {
    let tts = tts.clone();
    std::thread::spawn(move || {
        if let Err(e) = tts.engine.warm_up() {
            eprintln!("warm-up failed: {e}");
        }
    });
}

async fn ensure_arc(state: &AppState) -> anyhow::Result<Arc<Tts>> {
    match crate::android::TTS.peek() {
        Some(tts) => Ok(tts),
        None => {
            let tts = load_tts(state, None).await?;
            crate::android::TTS.install(tts.clone());
            warm(&tts);
            Ok(tts)
        }
    }
}

async fn lease_tts(state: &AppState) -> anyhow::Result<crate::android::Lease<'static, Tts>> {
    match crate::android::TTS.lease() {
        Some(lease) => Ok(lease),
        None => {
            let tts = load_tts(state, None).await?;
            Ok(crate::android::TTS.install_leased(tts))
        }
    }
}

pub fn startup_always_on(app: AppHandle) {
    let on = lock(&app.state::<AppState>().settings).always_on;
    if on {
        crate::android::set_always_pin(true);
        crate::android::engine_fgs(true);
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            if let Err(e) = ensure_arc(&state).await {
                let _ = app.emit("always-on-error", format!("{e:#}"));
            }
        });
    }
}

#[tauri::command]
pub async fn download_start(
    app: AppHandle,
    state: State<'_, AppState>,
    variant: Variant,
) -> CmdResult<()> {
    let cancel = {
        let mut guard = lock(&state.download);
        match &*guard {
            Some(_) => return Ok(()),
            None => {
                let token = CancelToken::default();
                *guard = Some(token.clone());
                token
            }
        }
    };
    let (models_dir, progress) = (state.models_dir.clone(), state.progress.clone());
    let fetching = [variant.spec(), &VOICES]
        .iter()
        .any(|s| file_size(&models_dir.join(s.name)) != Some(s.size));
    tauri::async_runtime::spawn(async move {
        if fetching {
            crate::android::download_fgs(true);
        }
        let result = run_download(app.clone(), models_dir, variant, cancel, progress).await;
        if fetching {
            crate::android::download_fgs(false);
        }
        let state = app.state::<AppState>();
        *lock(&state.download) = None;
        *lock(&state.progress) = None;
        match result {
            Ok(()) => {
                let _ = app.emit("download-done", ());
            }
            Err(e) => {
                let _ = app.emit("download-error", format!("{e:#}"));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn download_progress(state: State<'_, AppState>) -> CmdResult<Option<DownloadProgress>> {
    Ok(lock(&state.progress).clone())
}

#[tauri::command]
pub async fn download_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(token) = &*lock(&state.download) {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn install_status(state: State<'_, AppState>) -> CmdResult<InstallStatus> {
    Ok(install_status_inner(&state.models_dir))
}

#[tauri::command]
pub async fn engine_init(state: State<'_, AppState>, threads: ThreadConfig) -> CmdResult<()> {
    crate::android::TTS.clear();
    if crate::android::TTS.pinned() {
        let tts = load_tts(&state, Some(threads)).await?;
        crate::android::TTS.install(tts.clone());
        warm(&tts);
    }
    Ok(())
}

#[tauri::command]
pub async fn demo_synth(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    voice: String,
    speed: f32,
) -> CmdResult<Vec<u8>> {
    let tts = lease_tts(&state).await?;
    let wav = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let spec = VoiceSpec::parse(&voice)?;
        let _ = app.emit("synth-phase", "synthesizing");
        let mut samples = Vec::new();
        tts.synthesize_dsp(
            &text,
            &spec,
            speed.clamp(0.25, 4.0),
            &Chunker::default(),
            &CancelToken::default(),
            &mut |chunk| {
                samples.extend(chunk.samples);
                Ok(())
            },
        )?;
        Ok(Pcm {
            samples,
            sample_rate: SAMPLE_RATE,
        }
        .to_wav())
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(wav)
}

fn status_of(server: Option<&Server>) -> ServerStatus {
    ServerStatus {
        running: server.is_some(),
        addr: server.map(|s| s.addr().to_string()),
    }
}

#[tauri::command]
pub async fn server_start(state: State<'_, AppState>) -> CmdResult<ServerStatus> {
    let mut guard = state.server.lock().await;
    match &*guard {
        Some(server) => Ok(status_of(Some(server))),
        None => {
            crate::android::TTS.pin();
            let started = async {
                let tts = ensure_arc(&state).await?;
                let config = lock(&state.settings).server.clone();
                Server::start(tts, config).await
            }
            .await;
            match started {
                Ok(server) => {
                    let status = status_of(Some(&server));
                    *guard = Some(server);
                    crate::android::server_fgs(true);
                    Ok(status)
                }
                Err(e) => {
                    crate::android::TTS.unpin();
                    Err(e.into())
                }
            }
        }
    }
}

async fn stop_server(state: &AppState) -> anyhow::Result<()> {
    match state.server.lock().await.take() {
        Some(server) => {
            let result = server.stop().await;
            crate::android::TTS.unpin();
            crate::android::server_fgs(false);
            result
        }
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn server_stop(state: State<'_, AppState>) -> CmdResult<ServerStatus> {
    stop_server(&state).await?;
    Ok(status_of(None))
}

#[tauri::command]
pub async fn server_status(state: State<'_, AppState>) -> CmdResult<ServerStatus> {
    Ok(status_of(state.server.lock().await.as_ref()))
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(lock(&state.settings).clone())
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    let prev = lock(&state.settings).always_on;
    let json = serde_json::to_string_pretty(&settings)?;
    std::fs::write(state.config_dir.join("settings.json"), json)?;
    *lock(&state.flags) = settings.dsp;
    let now = settings.always_on;
    *lock(&state.settings) = settings;
    if now != prev {
        crate::android::set_always_pin(now);
        crate::android::engine_fgs(now);
        if now {
            match ensure_arc(&state).await {
                Ok(_) => {}
                Err(e) => {
                    crate::android::set_always_pin(false);
                    crate::android::engine_fgs(false);
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn dict_import(state: State<'_, AppState>, dict: CustomDict) -> CmdResult<usize> {
    let json = dict.to_json()?;
    std::fs::write(state.config_dir.join("dict.json"), json)?;
    let len = dict.0.len();
    *lock(&state.dict) = dict;
    Ok(len)
}

#[tauri::command]
pub async fn dict_export(state: State<'_, AppState>) -> CmdResult<CustomDict> {
    Ok(lock(&state.dict).clone())
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> CmdResult<u64> {
    let mut freed = 0;
    for entry in std::fs::read_dir(&state.models_dir)?.flatten() {
        let path = entry.path();
        let is_part = path.extension().map(|e| e == "part").unwrap_or(false);
        if is_part {
            freed += file_size(&path).unwrap_or(0);
            std::fs::remove_file(&path)?;
        }
    }
    Ok(freed)
}

#[tauri::command]
pub async fn delete_models(state: State<'_, AppState>) -> CmdResult<()> {
    stop_server(&state).await?;
    crate::android::TTS.clear();
    std::fs::remove_dir_all(&state.models_dir)?;
    std::fs::create_dir_all(&state.models_dir)?;
    Ok(())
}

#[tauri::command]
pub async fn battery_exemption() -> CmdResult<()> {
    crate::android::battery_exemption()?;
    Ok(())
}
