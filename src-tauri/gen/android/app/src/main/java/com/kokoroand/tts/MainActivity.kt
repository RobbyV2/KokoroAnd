package com.kokoroand.tts

import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  private var insetTop = 0f
  private var insetBottom = 0f

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    Native.nativeBindContext(applicationContext)
  }

  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(
      object {
        @JavascriptInterface fun top() = insetTop

        @JavascriptInterface fun bottom() = insetBottom
      },
      "AndroidInsets",
    )
    ViewCompat.setOnApplyWindowInsetsListener(webView) { _, insets ->
      val bars = WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      val i = insets.getInsets(bars)
      val density = resources.displayMetrics.density
      insetTop = i.top / density
      insetBottom = i.bottom / density
      webView.evaluateJavascript(
        "document.documentElement.style.setProperty('--inset-top','${insetTop}px');" +
          "document.documentElement.style.setProperty('--inset-bottom','${insetBottom}px')",
        null,
      )
      insets
    }
  }
}