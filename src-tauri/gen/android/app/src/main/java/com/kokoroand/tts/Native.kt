package com.kokoroand.tts

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.PowerManager
import android.provider.Settings

interface PcmSink {
  fun onPcm(pcm: ByteArray): Boolean
}

object Native {
  init {
    System.loadLibrary("app_lib")
  }

  external fun nativeBindContext(ctx: Context)

  external fun nativeInit(
    modelsDir: String,
    configDir: String,
  ): String?

  external fun nativeAlwaysOn(configDir: String): Boolean

  external fun nativeVoices(
    modelsDir: String,
    configDir: String,
  ): Array<String>?

  external fun nativeDefaultVoice(configDir: String): String

  external fun nativeSynthesize(
    modelsDir: String,
    configDir: String,
    text: String,
    voice: String,
    speed: Float,
    sink: PcmSink,
  ): String?

  external fun nativeCancel()

  external fun nativeServerStart(
    modelsDir: String,
    configDir: String,
  ): String?

  external fun nativeServerStop()

  @JvmStatic
  fun batteryExemption(ctx: Context) {
    val pm = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager
    if (pm.isIgnoringBatteryOptimizations(ctx.packageName)) return
    val intents =
      listOf(
        Intent(
          Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
          Uri.parse("package:${ctx.packageName}"),
        ),
        Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS),
      )
    for (intent in intents) {
      try {
        ctx.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        return
      } catch (_: ActivityNotFoundException) {
      }
    }
    throw ActivityNotFoundException("this device has no battery optimization settings screen")
  }
}