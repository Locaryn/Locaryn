package dev.locaryn.mobile

import android.graphics.Color
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  /**
   * La vue web ne peint pas son propre fond.
   *
   * Le lecteur de QR code dessine l'aperçu de la caméra *derrière* la vue web.
   * Rendre la page transparente en CSS ne suffit pas : la vue elle-même peint
   * un fond opaque par-dessus la caméra, et l'écran de lecture restait noir.
   *
   * Le reste du temps cela ne change rien — la page a son propre fond, opaque,
   * et c'est lui qu'on voit.
   */
  override fun onWebViewCreate(webView: WebView) {
    webView.setBackgroundColor(Color.TRANSPARENT)
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    // L'app est sombre partout. Forcer des barres sombres (icônes claires) pour
    // qu'elles ne deviennent jamais invisibles sur un appareil en thème clair.
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)

    // Android 15+ impose l'edge-to-edge : la WebView dessine sous l'horloge et
    // sous la barre de gestes. La replacer dans la zone sûre — le CSS ne peut
    // pas le faire, env(safe-area-inset-*) n'est pas peuplé par la WebView.
    //
    // Le clavier aussi : en edge-to-edge, `adjustResize` ne redimensionne plus
    // la fenêtre. Sans cette marge, la vue web gardait toute sa hauteur sous
    // le clavier, et Android faisait défiler la page entière pour montrer le
    // champ — la barre du haut ou le champ de saisie sortaient de l'écran.
    ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { _, insets ->
      window.decorView.findViewById<View>(android.R.id.content)?.let { content ->
        val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
        val clavier = insets.getInsets(WindowInsetsCompat.Type.ime())
        content.setPadding(
          bars.left,
          bars.top,
          bars.right,
          maxOf(bars.bottom, clavier.bottom),
        )
      }
      insets
    }
  }
}
