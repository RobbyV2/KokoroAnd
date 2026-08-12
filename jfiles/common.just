ROOT := `git rev-parse --show-toplevel`

export ANDROID_HOME := env_var_or_default("ANDROID_HOME", "/opt/homebrew/share/android-commandlinetools")
export ANDROID_SDK_ROOT := ANDROID_HOME
export NDK_HOME := ANDROID_HOME / "ndk" / shell('ls "$1" 2>/dev/null | sort -V | tail -1', ANDROID_HOME / "ndk")
export ANDROID_NDK_ROOT := NDK_HOME
export ANDROID_NDK_HOME := NDK_HOME
export JAVA_HOME := `echo "${JAVA_HOME:-$(brew --prefix openjdk@21 2>/dev/null)/libexec/openjdk.jdk/Contents/Home}"`

ADB_DEVICE := env_var_or_default("ADB_DEVICE", "")
adb_s := if ADB_DEVICE == "" { "" } else { "-s " + ADB_DEVICE }
KOKORO_PACKAGE := "com.kokoroand.tts"
