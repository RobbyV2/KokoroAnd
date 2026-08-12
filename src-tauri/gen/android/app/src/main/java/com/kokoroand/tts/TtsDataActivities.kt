package com.kokoroand.tts

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.speech.tts.TextToSpeech

class CheckVoiceData : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    val locales =
      arrayListOf(
        "eng-USA",
        "eng-GBR",
        "spa-ESP",
        "fra-FRA",
        "hin-IND",
        "ita-ITA",
        "jpn-JPN",
        "por-BRA",
        "zho-CHN",
      )
    val installed = java.io.File("${dataDir.absolutePath}/models/manifest.json").exists()
    val data =
      Intent().apply {
        putStringArrayListExtra(
          TextToSpeech.Engine.EXTRA_AVAILABLE_VOICES,
          if (installed) locales else arrayListOf(),
        )
        putStringArrayListExtra(
          TextToSpeech.Engine.EXTRA_UNAVAILABLE_VOICES,
          if (installed) arrayListOf() else locales,
        )
      }
    setResult(
      if (installed) TextToSpeech.Engine.CHECK_VOICE_DATA_PASS else TextToSpeech.Engine.CHECK_VOICE_DATA_FAIL,
      data,
    )
    finish()
  }
}

class GetSampleText : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    setResult(
      TextToSpeech.LANG_AVAILABLE,
      Intent()
        .putExtra(
          TextToSpeech.Engine.EXTRA_SAMPLE_TEXT,
          "Kokoro speaks with fifty four voices, entirely on this device.",
        ),
    )
    finish()
  }
}