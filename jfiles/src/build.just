import "../common.just"

# Desktop bundle
build:
  cd {{ROOT}} && cargo tauri build

# Android release APK; custom ORT with PGO by default, KOKORO_ORT=prebuilt links the pyke prebuilt
android-build:
  #!/usr/bin/env bash
  set -euo pipefail
  cd {{ROOT}}
  case "{{KOKORO_ORT}}" in
  prebuilt)
    cargo tauri android build --apk --target aarch64 ;;
  *)
    [ -f {{ORT_LIB}}/libonnxruntime_session.a ] || just src ort-build
    ORT_STRATEGY=system ORT_LIB_LOCATION={{ROOT}}/{{ORT_LIB}} \
    RUSTFLAGS="-C link-arg=-fprofile-use={{ROOT}}/{{ORT_PROFDATA}}" \
    cargo tauri android build --apk --target aarch64 ;;
  esac

# Install debug APK, ADB_DEVICE selects the device
android-install:
  cd {{ROOT}} && adb {{adb_s}} install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk

ORT_TAG := "v1.28.0"
ORT_COMMIT := "da9b5e364c465de65c49d91e696cd6485270757f"
ORT_NDK := ANDROID_HOME / "ndk/28.2.13676358"
ORT_OUT := "target/ort-android"
ORT_SRC := ORT_OUT / "onnxruntime"
ORT_LIB := ORT_OUT / "pgouse/Release"
ORT_PROFDATA := "jfiles/ort/merged.profdata"
ORT_PGO_WARN := "-Wno-profile-instr-unprofiled -Wno-profile-instr-out-of-date -Wno-backend-plugin"
KOKORO_ORT := env_var_or_default("KOKORO_ORT", "custom")

# Build ONNX Runtime static libs for aarch64-linux-android: -O3, LTO, PGO. Bit-exact vs the pyke prebuilt, 10-12% lower RTF.
ort-build: ort-clone
  cd {{ROOT}}/{{ORT_SRC}} && {{ROOT}}/.venv/bin/python tools/ci_build/build.py \
    --build_dir {{ROOT}}/{{ORT_OUT}}/pgouse --config Release --parallel --skip_tests --skip_submodule_sync \
    --android --android_sdk_path "$ANDROID_HOME" --android_ndk_path "{{ORT_NDK}}" \
    --android_abi arm64-v8a --android_api 29 --enable_lto --compile_no_warning_as_error \
    --cmake_extra_defines onnxruntime_BUILD_UNIT_TESTS=OFF \
      "CMAKE_C_FLAGS=-fprofile-use={{ROOT}}/{{ORT_PROFDATA}} {{ORT_PGO_WARN}}" \
      "CMAKE_CXX_FLAGS=-fprofile-use={{ROOT}}/{{ORT_PROFDATA}} {{ORT_PGO_WARN}}"

ort-clone:
  cd {{ROOT}} && [ -d {{ORT_SRC}} ] || git clone --depth 1 --branch {{ORT_TAG}} --recursive https://github.com/microsoft/onnxruntime {{ORT_SRC}}
  cd {{ROOT}}/{{ORT_SRC}} && [ "$(git rev-parse HEAD)" = "{{ORT_COMMIT}}" ]

# Regenerate {{ORT_PROFDATA}}: build instrumented libs, link a synthesis harness against
# {{ORT_OUT}}/pgogen/Release with RUSTFLAGS="-C link-arg=-fprofile-generate" plus
# -L native=<...>/pgogen/Release/model_package -l static=model_package (ort-sys omits that lib),
# run both models on device with LLVM_PROFILE_FILE=/data/local/tmp/prof-%p.profraw,
# pull the profraw files, then: <NDK>/llvm-profdata merge -o {{ORT_PROFDATA}} *.profraw
ort-pgo-gen: ort-clone
  cd {{ROOT}}/{{ORT_SRC}} && {{ROOT}}/.venv/bin/python tools/ci_build/build.py \
    --build_dir {{ROOT}}/{{ORT_OUT}}/pgogen --config Release --parallel --skip_tests --skip_submodule_sync \
    --android --android_sdk_path "$ANDROID_HOME" --android_ndk_path "{{ORT_NDK}}" \
    --android_abi arm64-v8a --android_api 29 --enable_lto \
    --cmake_extra_defines onnxruntime_BUILD_UNIT_TESTS=OFF \
      CMAKE_C_FLAGS=-fprofile-generate CMAKE_CXX_FLAGS=-fprofile-generate

# Compile check only
check-rust:
  cd {{ROOT}} && cargo check --workspace

# fmt-check + tests
check: fmt-check test

# Format and lint all code
fmt:
  cd {{ROOT}} && cargo clippy --workspace --all-targets --all-features --fix --allow-dirty -- -D warnings
  cd {{ROOT}} && cargo fmt --all
  cd {{ROOT}} && taplo fmt
  cd {{ROOT}} && bunx prettier --write "**/*.{ts,tsx,js,jsx,json,md,mjs,css,html}"

alias fix := fmt

# Check formatting and linting without changes
fmt-check:
  cd {{ROOT}} && cargo fmt --check --all
  cd {{ROOT}} && cargo clippy --workspace --all-targets --all-features -- -D warnings
  cd {{ROOT}} && taplo fmt --check
  cd {{ROOT}} && bunx prettier --check "**/*.{ts,tsx,js,jsx,json,md,mjs,css,html}"

# Remove build artifacts
clean:
  cd {{ROOT}} && cargo clean
  cd {{ROOT}} && rm -rf ui/dist
