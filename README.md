<img src="ui/logo.svg" alt="KokoroAnd logo" width="96" />

# KokoroAnd

Fast Kokoro TTS on Android but with G2P, system integration, and an API.

## Features

- 54 Kokoro voices, weighted voice mixing (`af_sarah(0.4)+af_nicole(0.6)`).
- Pure-Rust G2P with misaki parity
- Android system TTS engine (`TextToSpeechService`). Screen readers and reader apps can select Kokoro.
- OpenAI-compatible HTTP API on `127.0.0.1:8471`, kept alive by a foreground service on Android.
- Emotion tags (`[whisper]`, `[sad]`, ...) and smart punctuation pauses as optional DSP.
- Custom pronunciation dictionary with import and export.
- Two model variants, fp32 and fp16, selectable at onboarding and switchable in settings.

## Build

Install Bun, Rust, and just. For Android builds, install the Android SDK, the NDK, and JDK 21.
Set SDK path as `ANDROID_HOME`, and the NDK at `$ANDROID_HOME/ndk`. Then set `ADB_DEVICE` to an adb device.

```
just src install          # toolchain and ui dependencies
just src dev              # desktop app with hot reload
just src build            # desktop bundle
just src android-build    # Android release APK (arm64), optimized runtime
just src android-install  # install the APK on ADB_DEVICE
just src check            # format check, clippy, tests
just src fmt              # apply formatting and lint fixes
just src ort-build        # build the optimized runtime only
```

The default Android build compiles ONNX Runtime from source with -O3, LTO, and PGO. To skip the runtime build, link the prebuilt:

```
KOKORO_ORT=prebuilt just src android-build
```

Use `just src android-dev` for debug iteration; it links the prebuilt runtime.

## HTTP API

Enable the API in the app settings. The API is the same as Kokoro-FastAPI:

- `POST /v1/audio/speech` — synthesize; streams WAV by default.
- `GET /v1/audio/voices` — list voice ids.
- `GET /v1/models` — list model ids.
- `GET /health` — liveness.

```
curl http://127.0.0.1:8471/v1/audio/speech \
  -H 'Content-Type: application/json' \
  -d '{"model": "kokoro", "input": "Hello from Kokoro.", "voice": "af_bella", "speed": 1.0}' \
  -o hello.wav
```

`voice` accepts a single id, a mix expression, or an OpenAI alias (`alloy`, `nova`, ...). `speed` clamps to 0.25..4.0. `response_format` supports `wav` and `pcm`.

## Custom dictionary

The dictionary is a JSON object from wordform to pronunciation. A value is an IPA string, or an object keyed by POS tag with a `DEFAULT` entry:

```json
{
  "kokoro": "kˈOkəɹO",
  "read": { "DEFAULT": "ɹˈid", "VBD": "ɹˈɛd" }
}
```

Import and export the dictionary in settings. Inline overrides also work in any input text: `[kokoro](/kˈOkəɹO/)`.

## CI

CI lints/formats then publishes a release with a built apk. The APK signing key is `src-tauri/gen/android/app/sideload.jks`.

## Licenses

- Kokoro-82M model weights and the voice pack (hexgrad/Kokoro-82M via thewh1teagle/kokoro-onnx): Apache-2.0.
- misaki lexicon data embedded in kokoro-g2p (hexgrad/misaki): Apache-2.0.
- The app contains no espeak-ng and no other GPL component.
