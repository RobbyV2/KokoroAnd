use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::Tts;

pub struct Slot<T> {
    state: Mutex<SlotState<T>>,
}

struct SlotState<T> {
    value: Option<Arc<T>>,
    uses: usize,
    pins: usize,
}

impl<T> Slot<T> {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(SlotState {
                value: None,
                uses: 0,
                pins: 0,
            }),
        }
    }

    fn guard(&self) -> MutexGuard<'_, SlotState<T>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn peek(&self) -> Option<Arc<T>> {
        self.guard().value.clone()
    }

    pub fn lease(&self) -> Option<Lease<'_, T>> {
        let mut s = self.guard();
        match s.value.clone() {
            Some(value) => {
                s.uses += 1;
                Some(Lease { slot: self, value })
            }
            None => None,
        }
    }

    pub fn install(&self, value: Arc<T>) {
        self.guard().value = Some(value);
    }

    pub fn install_leased(&self, value: Arc<T>) -> Lease<'_, T> {
        let mut s = self.guard();
        s.value = Some(value.clone());
        s.uses += 1;
        Lease { slot: self, value }
    }

    pub fn pin(&self) {
        self.guard().pins += 1;
    }

    pub fn unpin(&self) {
        let mut s = self.guard();
        s.pins = s.pins.saturating_sub(1);
        if s.pins == 0 && s.uses == 0 {
            s.value = None;
        }
    }

    pub fn pinned(&self) -> bool {
        self.guard().pins > 0
    }

    pub fn clear(&self) {
        self.guard().value = None;
    }
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Lease<'a, T> {
    slot: &'a Slot<T>,
    value: Arc<T>,
}

impl<T> Deref for Lease<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> Drop for Lease<'_, T> {
    fn drop(&mut self) {
        let mut s = self.slot.guard();
        s.uses -= 1;
        if s.uses == 0 && s.pins == 0 {
            s.value = None;
        }
    }
}

pub static TTS: Slot<Tts> = Slot::new();

static ALWAYS_PIN: AtomicBool = AtomicBool::new(false);

pub fn set_always_pin(on: bool) {
    if ALWAYS_PIN.swap(on, Ordering::SeqCst) != on {
        match on {
            true => TTS.pin(),
            false => TTS.unpin(),
        }
    }
}

#[cfg(not(target_os = "android"))]
pub fn server_fgs(_active: bool) {}

#[cfg(not(target_os = "android"))]
pub fn engine_fgs(_active: bool) {}

#[cfg(not(target_os = "android"))]
pub fn download_fgs(_active: bool) {}

#[cfg(not(target_os = "android"))]
pub fn download_notify(_file: &str, _received: u64, _total: u64) {}

#[cfg(not(target_os = "android"))]
pub fn battery_exemption() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "android")]
pub use native::{battery_exemption, download_fgs, download_notify, engine_fgs, server_fgs};

#[cfg(target_os = "android")]
mod native {
    use std::path::Path;
    use std::sync::{Arc, Mutex, OnceLock};

    use jni::objects::{JClass, JObject, JString, JValue};
    use jni::sys::{jboolean, jfloat, jobjectArray, jstring};
    use jni::{JNIEnv, JavaVM};
    use kokoro_core::{CancelToken, Chunker, Error, Pcm, VoiceSpec};
    use kokoro_server::Server;

    use super::{Lease, TTS, set_always_pin};
    use crate::Tts;
    use crate::commands::{lock, read_dict, read_settings};

    static CANCEL: Mutex<Option<CancelToken>> = Mutex::new(None);
    static SERVER: Mutex<Option<Server>> = Mutex::new(None);
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

    fn build(models: &Path, config: &Path) -> anyhow::Result<Arc<Tts>> {
        let settings = read_settings(config);
        Ok(Arc::new(Tts::load(
            models,
            settings.threads,
            Arc::new(Mutex::new(read_dict(config))),
            Arc::new(Mutex::new(settings.dsp)),
        )?))
    }

    fn lease_tts(models: &Path, config: &Path) -> anyhow::Result<Lease<'static, Tts>> {
        match TTS.lease() {
            Some(lease) => Ok(lease),
            None => Ok(TTS.install_leased(build(models, config)?)),
        }
    }

    fn arc_tts(models: &Path, config: &Path) -> anyhow::Result<Arc<Tts>> {
        match TTS.peek() {
            Some(tts) => Ok(tts),
            None => {
                let tts = build(models, config)?;
                TTS.install(tts.clone());
                crate::commands::warm(&tts);
                Ok(tts)
            }
        }
    }

    fn jstr(env: &mut JNIEnv, s: &JString) -> String {
        env.get_string(s).map(|s| s.into()).unwrap_or_default()
    }

    fn jout(env: &mut JNIEnv, msg: Option<String>) -> jstring {
        match msg {
            None => std::ptr::null_mut(),
            Some(msg) => env
                .new_string(msg)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeInit(
        mut env: JNIEnv,
        _this: JObject,
        models: JString,
        config: JString,
    ) -> jstring {
        let (models, config) = (jstr(&mut env, &models), jstr(&mut env, &config));
        set_always_pin(read_settings(Path::new(&config)).always_on);
        let msg = match TTS.pinned() {
            true => arc_tts(Path::new(&models), Path::new(&config))
                .err()
                .map(|e| format!("{e:#}")),
            false => None,
        };
        jout(&mut env, msg)
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeAlwaysOn(
        mut env: JNIEnv,
        _this: JObject,
        config: JString,
    ) -> jboolean {
        let config = jstr(&mut env, &config);
        read_settings(Path::new(&config)).always_on as jboolean
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeVoices(
        mut env: JNIEnv,
        _this: JObject,
        models: JString,
        config: JString,
    ) -> jobjectArray {
        let (models, config) = (jstr(&mut env, &models), jstr(&mut env, &config));
        let lease = match lease_tts(Path::new(&models), Path::new(&config)) {
            Ok(lease) => lease,
            Err(_) => return std::ptr::null_mut(),
        };
        let build = |env: &mut JNIEnv| -> jni::errors::Result<jobjectArray> {
            let ids = lease.engine.voices();
            let arr =
                env.new_object_array(ids.len() as i32, "java/lang/String", JObject::null())?;
            for (i, id) in ids.iter().enumerate() {
                let s = env.new_string(&id.0)?;
                env.set_object_array_element(&arr, i as i32, s)?;
            }
            Ok(arr.into_raw())
        };
        build(&mut env).unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeDefaultVoice(
        mut env: JNIEnv,
        _this: JObject,
        config: JString,
    ) -> jstring {
        let config = jstr(&mut env, &config);
        let voice = read_settings(Path::new(&config)).voice;
        jout(&mut env, Some(voice))
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeSynthesize(
        mut env: JNIEnv,
        _this: JObject,
        models: JString,
        config: JString,
        text: JString,
        voice: JString,
        speed: jfloat,
        sink: JObject,
    ) -> jstring {
        let (models, config) = (jstr(&mut env, &models), jstr(&mut env, &config));
        let lease = match lease_tts(Path::new(&models), Path::new(&config)) {
            Ok(lease) => lease,
            Err(e) => return jout(&mut env, Some(format!("{e:#}"))),
        };
        let (text, voice) = (jstr(&mut env, &text), jstr(&mut env, &voice));
        let cancel = CancelToken::default();
        *lock(&CANCEL) = Some(cancel.clone());
        let result = (|| -> kokoro_core::Result<()> {
            let spec = VoiceSpec::parse(&voice)?;
            lease.synthesize_dsp(
                &text,
                &spec,
                speed.clamp(0.25, 4.0),
                &Chunker::default(),
                &cancel,
                &mut |pcm: Pcm| {
                    let bytes = pcm.to_s16le();
                    let arr = JObject::from(
                        env.byte_array_from_slice(&bytes)
                            .map_err(|e| Error::Model(e.to_string()))?,
                    );
                    let keep = env
                        .call_method(&sink, "onPcm", "([B)Z", &[JValue::Object(&arr)])
                        .and_then(|v| v.z());
                    let _ = env.delete_local_ref(arr);
                    match keep {
                        Ok(true) => Ok(()),
                        Ok(false) => Err(Error::Cancelled),
                        Err(e) => {
                            let _ = env.exception_clear();
                            Err(Error::Model(format!("sink: {e}")))
                        }
                    }
                },
            )
        })();
        *lock(&CANCEL) = None;
        let msg = match result {
            Ok(()) => None,
            Err(Error::Cancelled) => Some("cancelled".into()),
            Err(e) => Some(e.to_string()),
        };
        jout(&mut env, msg)
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeCancel(
        _env: JNIEnv,
        _this: JObject,
    ) {
        if let Some(cancel) = &*lock(&CANCEL) {
            cancel.cancel();
        }
    }

    fn runtime() -> anyhow::Result<&'static tokio::runtime::Runtime> {
        match RT.get() {
            Some(rt) => Ok(rt),
            None => {
                let _ = RT.set(tokio::runtime::Runtime::new()?);
                RT.get().ok_or_else(|| anyhow::anyhow!("runtime unset"))
            }
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeServerStart(
        mut env: JNIEnv,
        _this: JObject,
        models: JString,
        config: JString,
    ) -> jstring {
        let (models, config) = (jstr(&mut env, &models), jstr(&mut env, &config));
        let start = || -> anyhow::Result<()> {
            let mut guard = lock(&SERVER);
            match &*guard {
                Some(_) => Ok(()),
                None => {
                    TTS.pin();
                    let run = || -> anyhow::Result<Server> {
                        let tts = arc_tts(Path::new(&models), Path::new(&config))?;
                        let cfg = read_settings(Path::new(&config)).server;
                        Ok(runtime()?.block_on(Server::start(tts, cfg))?)
                    };
                    match run() {
                        Ok(server) => {
                            *guard = Some(server);
                            Ok(())
                        }
                        Err(e) => {
                            TTS.unpin();
                            Err(e)
                        }
                    }
                }
            }
        };
        let msg = start().err().map(|e| format!("{e:#}"));
        jout(&mut env, msg)
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeServerStop(
        _env: JNIEnv,
        _this: JObject,
    ) {
        let server = lock(&SERVER).take();
        if let Some(server) = server {
            TTS.unpin();
            let stop = || -> anyhow::Result<()> {
                runtime()?.block_on(server.stop())?;
                Ok(())
            };
            if let Err(e) = stop() {
                eprintln!("server stop failed: {e:#}");
            }
        }
    }

    static BIND: std::sync::Once = std::sync::Once::new();

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_kokoroand_tts_Native_nativeBindContext(
        env: JNIEnv,
        _this: JObject,
        ctx: JObject,
    ) {
        BIND.call_once(|| {
            let bind = || -> anyhow::Result<()> {
                let vm = env.get_java_vm()?;
                let global = env.new_global_ref(&ctx)?;
                let ptr = global.as_obj().as_raw();
                std::mem::forget(global);
                unsafe {
                    ndk_context::initialize_android_context(
                        vm.get_java_vm_pointer().cast(),
                        ptr.cast(),
                    )
                };
                Ok(())
            };
            if let Err(e) = bind() {
                eprintln!("bind context failed: {e:#}");
            }
        });
    }

    fn take_exception(env: &mut JNIEnv) -> Option<String> {
        if !env.exception_check().unwrap_or(false) {
            return None;
        }
        let _ = env.exception_describe();
        let throwable = env.exception_occurred().ok()?;
        env.exception_clear().ok()?;
        let text = env
            .call_method(&throwable, "toString", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        env.get_string(&JString::from(text)).ok().map(|s| s.into())
    }

    fn with_ctx<T>(
        f: impl FnOnce(&mut JNIEnv, &JObject) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let result = f(&mut env, &context);
        match take_exception(&mut env) {
            Some(msg) => Err(anyhow::anyhow!(msg)),
            None => result,
        }
    }

    fn load_class<'l>(
        env: &mut JNIEnv<'l>,
        ctx: &JObject,
        name: &str,
    ) -> anyhow::Result<JClass<'l>> {
        let loader = env
            .call_method(ctx, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .l()?;
        let jname = JObject::from(env.new_string(name)?);
        let class = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&jname)],
            )?
            .l()?;
        Ok(JClass::from(class))
    }

    fn fgs_toggle(class: &str, active: bool) {
        let result = with_ctx(|env, ctx| {
            let cls = load_class(env, ctx, class)?;
            env.call_static_method(
                &cls,
                "toggle",
                "(Landroid/content/Context;Z)V",
                &[JValue::Object(ctx), JValue::Bool(active as u8)],
            )?;
            Ok(())
        });
        if let Err(e) = result {
            eprintln!("{class} toggle failed: {e:#}");
        }
    }

    pub fn server_fgs(active: bool) {
        fgs_toggle("com.kokoroand.tts.ServerFgs", active);
    }

    pub fn engine_fgs(active: bool) {
        fgs_toggle("com.kokoroand.tts.EngineFgs", active);
    }

    pub fn download_fgs(active: bool) {
        fgs_toggle("com.kokoroand.tts.DownloadFgs", active);
    }

    pub fn download_notify(file: &str, received: u64, total: u64) {
        let _ = with_ctx(|env, ctx| {
            let cls = load_class(env, ctx, "com.kokoroand.tts.DownloadFgs")?;
            let jfile = JObject::from(env.new_string(file)?);
            env.call_static_method(
                &cls,
                "progress",
                "(Landroid/content/Context;Ljava/lang/String;JJ)V",
                &[
                    JValue::Object(ctx),
                    JValue::Object(&jfile),
                    JValue::Long(received as i64),
                    JValue::Long(total as i64),
                ],
            )?;
            Ok(())
        });
    }

    pub fn battery_exemption() -> anyhow::Result<()> {
        with_ctx(|env, ctx| {
            let cls = load_class(env, ctx, "com.kokoroand.tts.Native")?;
            env.call_static_method(
                &cls,
                "batteryExemption",
                "(Landroid/content/Context;)V",
                &[JValue::Object(ctx)],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::Slot;

    struct Probe(Arc<AtomicUsize>);

    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn probe() -> (Arc<AtomicUsize>, Arc<Probe>) {
        let drops = Arc::new(AtomicUsize::new(0));
        (drops.clone(), Arc::new(Probe(drops.clone())))
    }

    #[test]
    fn releases_at_zero_uses() {
        let slot = Slot::new();
        let (drops, value) = probe();
        let a = slot.install_leased(value);
        let b = slot.lease().unwrap();
        drop(a);
        assert!(slot.peek().is_some());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(b);
        assert!(slot.peek().is_none());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(slot.lease().is_none());
    }

    #[test]
    fn pins_hold_value() {
        let slot = Slot::new();
        let (drops, value) = probe();
        slot.pin();
        slot.install(value);
        drop(slot.lease().unwrap());
        assert!(slot.peek().is_some());
        slot.pin();
        slot.unpin();
        assert!(slot.peek().is_some());
        slot.unpin();
        assert!(slot.peek().is_none());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unpin_waits_for_active_use() {
        let slot = Slot::new();
        let (drops, value) = probe();
        slot.pin();
        let lease = slot.install_leased(value);
        slot.unpin();
        assert!(slot.peek().is_some());
        drop(lease);
        assert!(slot.peek().is_none());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_load_release() {
        let slot = Slot::new();
        let drops = Arc::new(AtomicUsize::new(0));
        let installs = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..200 {
                        let lease = match slot.lease() {
                            Some(lease) => lease,
                            None => {
                                installs.fetch_add(1, Ordering::SeqCst);
                                slot.install_leased(Arc::new(Probe(drops.clone())))
                            }
                        };
                        drop(lease);
                    }
                });
            }
        });
        assert!(slot.peek().is_none());
        assert_eq!(
            drops.load(Ordering::SeqCst),
            installs.load(Ordering::SeqCst)
        );
    }
}
