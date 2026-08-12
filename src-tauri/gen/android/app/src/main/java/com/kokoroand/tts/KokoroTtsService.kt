package com.kokoroand.tts

import android.media.AudioFormat
import android.speech.tts.SynthesisCallback
import android.speech.tts.SynthesisRequest
import android.speech.tts.TextToSpeech
import android.speech.tts.TextToSpeechService
import android.speech.tts.Voice
import android.util.Log
import java.util.Locale

class KokoroTtsService : TextToSpeechService() {
  private val configDir: String by lazy { dataDir.absolutePath }
  private val modelsDir: String by lazy { "$configDir/models" }

  @Volatile private var current = arrayOf("eng", "USA", "")

  private fun installed(): Boolean = java.io.File("$modelsDir/manifest.json").exists()

  private fun voiceIds(): List<String> = if (installed()) VOICE_IDS else emptyList()

  private fun configuredVoice(): String =
    runCatching {
      org.json.JSONObject(java.io.File("$configDir/settings.json").readText()).optString("voice")
    }.getOrDefault("")

  private fun localeOf(id: String): Locale = LOCALES[id.firstOrNull()] ?: Locale("en", "US")

  private fun voiceName(id: String): String {
    val loc = localeOf(id)
    return "${loc.language}-${loc.country.lowercase()}-$id"
  }

  private fun idOf(name: String): String = name.split("-", limit = 3).last()

  override fun onIsLanguageAvailable(
    lang: String?,
    country: String?,
    variant: String?,
  ): Int {
    if (!installed()) return TextToSpeech.LANG_NOT_SUPPORTED
    val l = ISO3_LANG[lang?.lowercase()] ?: lang?.lowercase() ?: ""
    val c = ISO3_COUNTRY[country?.uppercase()] ?: country?.uppercase().orEmpty()
    val hits = LOCALES.values.filter { it.language == l }
    return when {
      hits.isEmpty() -> TextToSpeech.LANG_NOT_SUPPORTED
      hits.any { it.country == c } -> TextToSpeech.LANG_COUNTRY_AVAILABLE
      else -> TextToSpeech.LANG_AVAILABLE
    }
  }

  override fun onGetLanguage(): Array<String> = current

  override fun onLoadLanguage(
    lang: String?,
    country: String?,
    variant: String?,
  ): Int {
    val avail = onIsLanguageAvailable(lang, country, variant)
    if (avail != TextToSpeech.LANG_NOT_SUPPORTED) {
      current = arrayOf(lang.orEmpty(), country.orEmpty(), "")
    }
    return avail
  }

  override fun onGetVoices(): MutableList<Voice> =
    voiceIds()
      .map {
        Voice(voiceName(it), localeOf(it), Voice.QUALITY_HIGH, Voice.LATENCY_NORMAL, false, setOf())
      }.toMutableList()

  override fun onIsValidVoiceName(name: String?): Int =
    if (name != null && voiceIds().contains(idOf(name))) {
      TextToSpeech.SUCCESS
    } else {
      TextToSpeech.ERROR
    }

  override fun onLoadVoice(name: String?): Int = onIsValidVoiceName(name)

  override fun onGetDefaultVoiceNameFor(
    lang: String?,
    country: String?,
    variant: String?,
  ): String {
    val l = ISO3_LANG[lang?.lowercase()] ?: lang?.lowercase().orEmpty()
    val chosen = configuredVoice()
    val plain = chosen.isNotEmpty() && chosen.all { it.isLetter() || it == '_' }
    if (plain && localeOf(chosen).language == l) return voiceName(chosen)
    return voiceName(DEFAULTS[l] ?: "af_bella")
  }

  override fun onSynthesizeText(
    request: SynthesisRequest,
    callback: SynthesisCallback,
  ) {
    if (!installed()) {
      callback.error(TextToSpeech.ERROR_NOT_INSTALLED_YET)
      return
    }
    val name =
      request.voiceName?.takeIf { n -> VOICE_IDS.contains(idOf(n)) }
        ?: onGetDefaultVoiceNameFor(request.language, request.country, request.variant)
    val text = request.charSequenceText?.toString().orEmpty()
    val speed = (request.speechRate / 100f).coerceIn(0.25f, 4f)
    if (text.isBlank()) {
      callback.start(24000, AudioFormat.ENCODING_PCM_16BIT, 1)
      callback.done()
      return
    }
    var started = false
    val sink =
      object : PcmSink {
        override fun onPcm(pcm: ByteArray): Boolean {
          if (!started) {
            callback.start(24000, AudioFormat.ENCODING_PCM_16BIT, 1)
            started = true
          }
          var off = 0
          while (off < pcm.size) {
            val n = minOf(callback.maxBufferSize, pcm.size - off)
            if (callback.audioAvailable(pcm, off, n) != TextToSpeech.SUCCESS) return false
            off += n
          }
          return true
        }
      }
    when (val err = Native.nativeSynthesize(modelsDir, configDir, text, idOf(name), speed, sink)) {
      null,
      "cancelled",
      -> callback.done()
      else -> {
        Log.e("KokoroTtsService", err)
        callback.error(TextToSpeech.ERROR_SYNTHESIS)
      }
    }
  }

  override fun onStop() {
    Native.nativeCancel()
  }

  companion object {
    private val VOICE_IDS =
      listOf(
        "af_alloy",
        "af_aoede",
        "af_bella",
        "af_heart",
        "af_jessica",
        "af_kore",
        "af_nicole",
        "af_nova",
        "af_river",
        "af_sarah",
        "af_sky",
        "am_adam",
        "am_echo",
        "am_eric",
        "am_fenrir",
        "am_liam",
        "am_michael",
        "am_onyx",
        "am_puck",
        "am_santa",
        "bf_alice",
        "bf_emma",
        "bf_isabella",
        "bf_lily",
        "bm_daniel",
        "bm_fable",
        "bm_george",
        "bm_lewis",
        "ef_dora",
        "em_alex",
        "em_santa",
        "ff_siwis",
        "hf_alpha",
        "hf_beta",
        "hm_omega",
        "hm_psi",
        "if_sara",
        "im_nicola",
        "jf_alpha",
        "jf_gongitsune",
        "jf_nezumi",
        "jf_tebukuro",
        "jm_kumo",
        "pf_dora",
        "pm_alex",
        "pm_santa",
        "zf_xiaobei",
        "zf_xiaoni",
        "zf_xiaoxiao",
        "zf_xiaoyi",
        "zm_yunjian",
        "zm_yunxi",
        "zm_yunxia",
        "zm_yunyang",
      )
    private val LOCALES =
      mapOf(
        'a' to Locale("en", "US"),
        'b' to Locale("en", "GB"),
        'e' to Locale("es", "ES"),
        'f' to Locale("fr", "FR"),
        'h' to Locale("hi", "IN"),
        'i' to Locale("it", "IT"),
        'j' to Locale("ja", "JP"),
        'p' to Locale("pt", "BR"),
        'z' to Locale("zh", "CN"),
      )
    private val ISO3_LANG =
      mapOf(
        "eng" to "en",
        "spa" to "es",
        "fra" to "fr",
        "fre" to "fr",
        "hin" to "hi",
        "ita" to "it",
        "jpn" to "ja",
        "por" to "pt",
        "zho" to "zh",
        "cmn" to "zh",
      )
    private val ISO3_COUNTRY =
      mapOf(
        "USA" to "US",
        "GBR" to "GB",
        "ESP" to "ES",
        "FRA" to "FR",
        "IND" to "IN",
        "ITA" to "IT",
        "JPN" to "JP",
        "BRA" to "BR",
        "CHN" to "CN",
      )
    private val DEFAULTS =
      mapOf(
        "en" to "af_bella",
        "es" to "ef_dora",
        "fr" to "ff_siwis",
        "hi" to "hf_alpha",
        "it" to "if_sara",
        "ja" to "jf_alpha",
        "pt" to "pf_dora",
        "zh" to "zf_xiaobei",
      )
  }
}