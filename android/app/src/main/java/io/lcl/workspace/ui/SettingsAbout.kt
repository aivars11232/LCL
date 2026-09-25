package io.lcl.workspace.ui

import android.net.Uri
import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.lcl.workspace.AppContainer
import io.lcl.workspace.BuildConfig
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.data.Theme
import io.lcl.workspace.editor.CodeEditor
import io.lcl.workspace.editor.EditorState
import io.lcl.workspace.remote.Protocol
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.str
import io.lcl.workspace.workspace.OpenDocument
import io.lcl.workspace.workspace.WorkspaceController
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Composable
private fun Title(text: String, onBack: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        TextButton(onClick = onBack, Modifier.testTag("back")) { Text("Back") }
        Text(text, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
    }
}

/** Presentation on this device only. None of it reaches the PC or changes a document. */
@Composable
fun SettingsScreen(settings: AppSettings, onChange: (AppSettings) -> Unit, onBack: () -> Unit) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Title("Settings", onBack)
        Text("Appearance", fontWeight = FontWeight.SemiBold)
        for ((theme, label) in listOf(Theme.SYSTEM to "System", Theme.DARK to "Dark", Theme.LIGHT to "Light")) {
            Row(
                Modifier.fillMaxWidth().selectable(selected = settings.theme == theme, onClick = { onChange(settings.copy(theme = theme)) }).testTag("theme_${label.lowercase()}"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RadioButton(selected = settings.theme == theme, onClick = { onChange(settings.copy(theme = theme)) })
                Text(label)
            }
        }
        Text("Editor", fontWeight = FontWeight.SemiBold)
        Text("Font size: ${settings.fontSize} sp")
        Slider(
            value = settings.fontSize.toFloat(),
            onValueChange = { onChange(settings.copy(fontSize = it.toInt())) },
            valueRange = AppSettings.MIN_FONT.toFloat()..AppSettings.MAX_FONT.toFloat(),
            steps = AppSettings.MAX_FONT - AppSettings.MIN_FONT - 1,
            modifier = Modifier.testTag("font_size"),
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Show line numbers", Modifier.weight(1f))
            Switch(checked = settings.lineNumbers, onCheckedChange = { onChange(settings.copy(lineNumbers = it)) }, modifier = Modifier.testTag("line_numbers"))
        }
        Text(
            "Documents are always indented with spaces: the Indent button and a keyboard's Tab key insert four. " +
                "These settings stay on this device.",
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
fun AboutScreen(container: AppContainer, connection: ConnectionState, onBack: () -> Unit) {
    val ui by container.workspace.ui.collectAsState()
    val about: JsonObject? = ui.about
    val pc = connection.pcOrNull
    val deviceFingerprint = remember(pc?.keyAlias) {
        pc?.let { runCatching { container.identities.load(it.keyAlias)?.fingerprint }.getOrNull() }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Title("About", onBack)
        @Composable
        fun row(label: String, value: String?) {
            Text(label, style = MaterialTheme.typography.labelMedium)
            Text(value ?: "—", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("about:$label"))
        }
        row("App", "LCL for Android ${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})")
        row("Remote protocol", Protocol.VERSION)
        row("Connection", describe(connection))
        row("PC", pc?.let { "${it.name} (${it.pcId})" })
        row("PC fingerprint", pc?.fingerprint)
        row("This device's fingerprint for it", deviceFingerprint)
        row("PC service", about?.str("service"))
        row("PC engine protocol", about?.str("engine_protocol"))
        val core = about?.obj("core")
        row("LCL Core 0.1", core?.let { "${it.str("formal_version")} · ${it.str("authority")}\n${it.str("identity_digest")}" })
        val localized = about?.obj("localized")
        row("LCL Core 0.2", localized?.let { "${it.str("formal_version")} · ${it.str("authority")}\n${it.str("identity_digest")}" } ?: "not installed on this PC")
        row("Android", "${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}) · ${Build.MANUFACTURER} ${Build.MODEL}")
        row("ABIs", Build.SUPPORTED_ABIS.joinToString())
        if (about == null) Text("PC details appear once connected.", style = MaterialTheme.typography.bodySmall)
    }
}

/**
 * A `.lcl` or `.lcl.txt` file opened from elsewhere on this phone. It is shown
 * as it is — nothing on the phone judges LCL — and the connected PC can check
 * or inspect it with its engine.
 */
@Composable
fun LocalDocumentScreen(container: AppContainer, uri: Uri, settings: AppSettings, onBack: () -> Unit) {
    val context = LocalContext.current
    var text by remember { mutableStateOf<String?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var verdict by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val name = uri.lastPathSegment?.substringAfterLast('/') ?: "document.lcl"
    LaunchedEffect(uri) {
        runCatching {
            withContext(Dispatchers.IO) {
                context.contentResolver.openInputStream(uri)!!.use { stream ->
                    // Read at most 4 MB; `readNBytes` would need Android 13.
                    val limit = 4 * 1024 * 1024
                    val bytes = java.io.ByteArrayOutputStream()
                    val buffer = ByteArray(64 * 1024)
                    while (true) {
                        val read = stream.read(buffer)
                        if (read < 0) break
                        bytes.write(buffer, 0, read)
                        require(bytes.size() <= limit) { "The file is larger than 4 MB." }
                    }
                    bytes.toString(Charsets.UTF_8.name())
                }
            }
        }.onSuccess { text = it }.onFailure { problem = it.message }
    }
    Column(Modifier.fillMaxSize().padding(8.dp)) {
        Title(name, onBack)
        problem?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        val loaded = text ?: return@Column
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (op in listOf("check", "inspect")) {
                OutlinedButton(
                    onClick = {
                        scope.launch {
                            val project = container.workspace.ui.value.project
                                ?: return@launch run { verdict = "Connect to a PC to check this document." }
                            val reply = runCatching {
                                container.connection.request(op, buildJsonObject {
                                    put("project", project.id)
                                    put("document", name)
                                    put("text", loaded)
                                })
                            }.getOrElse { return@launch run { verdict = it.message } }
                            verdict = if (reply.ok) {
                                val diagnostics = WorkspaceController.diagnosticSpans(reply.obj, name).size
                                "${op.replaceFirstChar { it.uppercase() }} on the PC: ${reply.obj.str("outcome")} at ${reply.obj.str("reached")}, $diagnostics diagnostic(s)"
                            } else {
                                reply.error
                            }
                        }
                    },
                    enabled = connection(container) is ConnectionState.Connected,
                ) { Text("${op.replaceFirstChar { it.uppercase() }} on PC") }
            }
        }
        verdict?.let { Text(it, Modifier.padding(4.dp)) }
        val editor = remember { EditorState() }
        CodeEditor(
            document = OpenDocument("local", name, loaded, loaded, ""),
            state = editor,
            onTextChange = {},
            readOnly = true,
            fontSize = settings.fontSize,
            lineNumbers = settings.lineNumbers,
            modifier = Modifier.weight(1f).fillMaxWidth(),
        )
    }
}

@Composable
private fun connection(container: AppContainer): ConnectionState = container.connection.state.collectAsState().value
