import "../common.just"

# Desktop dev app with hot reload
dev:
  cd {{ROOT}} && cargo tauri dev

# Android dev with hot reload, ADB_DEVICE selects the device
android-dev:
  cd {{ROOT}} && {{ if ADB_DEVICE == "" { "" } else { "ANDROID_SERIAL=" + ADB_DEVICE } }} cargo tauri android dev

# Vite dev server only
ui:
  cd {{ROOT}}/ui && bun run dev
