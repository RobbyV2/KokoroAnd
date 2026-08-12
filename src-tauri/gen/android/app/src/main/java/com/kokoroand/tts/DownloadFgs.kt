package com.kokoroand.tts

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

class DownloadFgs : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    running = true
  }

  override fun onStartCommand(
    intent: Intent?,
    flags: Int,
    startId: Int,
  ): Int {
    Notif.ensureChannel(this)
    val n = build(this, "Starting").build()
    if (Build.VERSION.SDK_INT >= 29) {
      startForeground(Notif.DOWNLOAD_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    } else {
      startForeground(Notif.DOWNLOAD_ID, n)
    }
    if (intent?.getBooleanExtra("stop", false) == true) {
      stopForeground(STOP_FOREGROUND_REMOVE)
      stopSelf()
    }
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    running = false
    super.onDestroy()
  }

  companion object {
    @Volatile private var running = false
    private var lastNotify = 0L

    private fun build(
      ctx: Context,
      text: String,
    ) = Notif.build(ctx, android.R.drawable.stat_sys_download, "Downloading voice models", text)

    @JvmStatic
    fun toggle(
      ctx: Context,
      on: Boolean,
    ) {
      val i = Intent(ctx, DownloadFgs::class.java)
      if (on) ctx.startForegroundService(i) else ctx.startService(i.putExtra("stop", true))
    }

    @JvmStatic
    fun progress(
      ctx: Context,
      file: String,
      received: Long,
      total: Long,
    ) {
      if (!running) return
      val now = System.currentTimeMillis()
      if (now - lastNotify < 500) return
      lastNotify = now
      val n =
        build(ctx, file)
          .apply {
            if (total > 0) {
              setProgress(1000, ((received * 1000) / total).toInt(), false)
            } else {
              setProgress(0, 0, true)
            }
          }.build()
      Notif.manager(ctx).notify(Notif.DOWNLOAD_ID, n)
    }
  }
}