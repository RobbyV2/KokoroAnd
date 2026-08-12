package com.kokoroand.tts

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent

object Notif {
  const val CHANNEL = "kokoro"
  const val SERVER_ID = 1
  const val DOWNLOAD_ID = 2
  const val ENGINE_ID = 3

  fun manager(ctx: Context) = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

  fun ensureChannel(ctx: Context) {
    manager(ctx).createNotificationChannel(
      NotificationChannel(CHANNEL, "Background tasks", NotificationManager.IMPORTANCE_LOW),
    )
  }

  fun build(
    ctx: Context,
    icon: Int,
    title: String,
    text: String,
  ): Notification.Builder {
    val open =
      PendingIntent.getActivity(
        ctx,
        0,
        Intent(ctx, MainActivity::class.java),
        PendingIntent.FLAG_IMMUTABLE,
      )
    return Notification
      .Builder(ctx, CHANNEL)
      .setSmallIcon(icon)
      .setContentTitle(title)
      .setContentText(text)
      .setContentIntent(open)
      .setOngoing(true)
  }
}