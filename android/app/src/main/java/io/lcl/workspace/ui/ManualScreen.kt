package io.lcl.workspace.ui

import android.annotation.SuppressLint
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import io.lcl.workspace.manual.ManualSnapshot

/** Where the reader was in the manual, for as long as the app's process lives. */
object ManualPosition {
    @Volatile var saved: String = ""
}

/**
 * The Users Manual tab: the packaged manual, read with the same viewer the desktop workspace uses,
 * entirely offline. Read-only: the page can reach nothing but the packaged snapshot, and nothing it
 * does reaches the PC, a document or the pairing.
 */
@SuppressLint("SetJavaScriptEnabled") // the viewer is packaged, local and loads nothing else
@Composable
fun ManualScreen() {
    val context = LocalContext.current
    val snapshot = remember {
        val assets = context.assets
        runCatching {
            ManualSnapshot.load(assets.list("manual")?.toList() ?: emptyList()) { name ->
                assets.open("manual/$name").use { it.readBytes() }
            }
        }
            .getOrNull()
    }
    var status by remember { mutableStateOf("Loading the manual…") }
    Column(Modifier.fillMaxSize()) {
        Text(
            when {
                snapshot == null -> "The packaged manual could not be read."
                !snapshot.matchesManifest -> "The packaged manual does not match its manifest."
                else -> status
            },
            style = MaterialTheme.typography.bodySmall,
            modifier =
                Modifier.fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 2.dp)
                    .testTag("manual_status"),
        )
        if (snapshot == null) return@Column
        AndroidView(
            modifier = Modifier.fillMaxSize().testTag("manual_view"),
            factory = { viewContext ->
                WebView(viewContext).apply {
                    settings.javaScriptEnabled = true
                    settings.allowFileAccess = false
                    settings.allowContentAccess = false
                    settings.domStorageEnabled = false
                    // No page is ever loaded from anywhere: every navigation
                    // other than moving within the manual is refused.
                    webViewClient =
                        object : WebViewClient() {
                            override fun shouldOverrideUrlLoading(
                                view: WebView,
                                request: WebResourceRequest,
                            ): Boolean = true
                        }
                    addJavascriptInterface(
                        ManualBridge(snapshot) { loaded -> post { status = loaded } },
                        "LclManual",
                    )
                    val page =
                        viewContext.assets
                            .open("manual/manual.html")
                            .use { it.readBytes() }
                            .toString(Charsets.UTF_8)
                            .replace("{{TOKEN}}", "")
                            .replace("{{Q}}", "")
                    loadDataWithBaseURL(
                        "file:///android_asset/manual/",
                        page,
                        "text/html",
                        "utf-8",
                        null,
                    )
                }
            },
        )
    }
}

/** What the viewer may ask of the app: the snapshot, and where the reader was. */
private class ManualBridge(
    private val snapshot: ManualSnapshot,
    private val onLoaded: (String) -> Unit,
) {
    @JavascriptInterface fun snapshot(): String = snapshot.json()

    @JavascriptInterface fun recall(): String = ManualPosition.saved

    @JavascriptInterface
    fun remember(state: String) {
        if (state.length <= 64 * 1024) ManualPosition.saved = state
    }

    @JavascriptInterface
    fun loaded(info: String) {
        val parts = info.split(' ')
        onLoaded(
            "Users Manual ${parts.getOrNull(0) ?: ""} · ${parts.getOrNull(2) ?: "?"} files · offline · ${parts.getOrNull(1)?.take(12) ?: ""}"
        )
    }
}
