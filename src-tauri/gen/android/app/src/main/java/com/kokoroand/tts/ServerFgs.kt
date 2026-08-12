package com.kokoroand.tts

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

class ServerFgs : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(
    intent: Intent?,
    flags: Int,
    startId: Int,
  ): Int {
    Notif.ensureChannel(this)
    val n =
      Notif
        .build(
          this,
          android.R.drawable.stat_notify_sync_noanim,
          "Kokoro API server",
          "OpenAI-compatible TTS API is running",
        ).build()
    if (Build.VERSION.SDK_INT >= 34) {
      startForeground(Notif.SERVER_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
    } else {
      startForeground(Notif.SERVER_ID, n)
    }
    if (intent == null) {
      Thread {
        Native.nativeServerStart("${dataDir.absolutePath}/models", dataDir.absolutePath)
      }.start()
    }
    return START_STICKY
  }

  override fun onDestroy() {
    Native.nativeServerStop()
    super.onDestroy()
  }

  companion object {
    @JvmStatic
    fun toggle(
      ctx: Context,
      on: Boolean,
    ) {
      val i = Intent(ctx, ServerFgs::class.java)
      if (on) ctx.startForegroundService(i) else ctx.stopService(i)
    }
  }
}