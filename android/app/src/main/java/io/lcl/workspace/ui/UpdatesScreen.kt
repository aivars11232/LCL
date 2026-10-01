package io.lcl.workspace.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import io.lcl.workspace.AppContainer
import io.lcl.workspace.BuildConfig
import io.lcl.workspace.update.Phase
import io.lcl.workspace.update.ProductVersion
import io.lcl.workspace.update.UpdateUi
import java.text.DateFormat
import java.util.Date

private fun stateLabel(ui: UpdateUi) =
    when (ui.phase) {
        Phase.Idle -> "Not checked yet"
        Phase.Checking -> "Checking"
        Phase.UpToDate -> "Up to date"
        Phase.Available -> "Update available"
        Phase.Downloading -> "Downloading and verifying"
        Phase.NeedsPermission -> "Verified — waiting for permission to install"
        Phase.Installing ->
            if (ui.waitingForConfirmation) "Waiting for your confirmation" else "Installing"
        Phase.Failed -> "Failed"
        Phase.Offline -> "Could not check: offline"
        Phase.NotConfigured -> "Updates are not set up in this build"
    }

private fun problemLabel(kind: String) =
    when (kind) {
        "offline" -> "No network"
        "invalid" -> "Invalid update"
        "verification" -> "Verification failed"
        "download" -> "Download failed"
        "install" -> "Installation failed"
        "unsupported" -> "Cannot update automatically"
        "cancelled" -> "Cancelled"
        "not_configured" -> "Not set up"
        else -> "Problem"
    }

private fun size(bytes: Long) =
    if (bytes >= 1_048_576) "%.1f MB".format(bytes / 1_048_576.0) else "${(bytes + 1023) / 1024} KB"

/** Updates: works without a PC, and never interrupts anything else. */
@Composable
fun UpdatesScreen(container: AppContainer, onBack: () -> Unit) {
    val updates = container.updates
    val ui by updates.ui.collectAsState()
    // Back from Android's "Install unknown apps" setting: continue if allowed.
    LifecycleResumeEffect(Unit) {
        updates.resume()
        onPauseOrDispose {}
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onBack, Modifier.testTag("back")) { Text("Back") }
            Text(
                "Updates",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
            )
        }
        Text(
            "Installed version: LCL ${ProductVersion.shown(BuildConfig.VERSION_NAME)}",
            Modifier.testTag("update_installed"),
        )
        Text(
            "Last update check: " +
                (ui.lastCheck?.let { DateFormat.getDateTimeInstance().format(Date(it)) } ?: "never")
        )
        Text(
            "State: ${stateLabel(ui)}",
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.testTag("update_state"),
        )
        ui.problem?.let { (kind, message) ->
            Text(
                "${problemLabel(kind)}: $message",
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.testTag("update_problem"),
            )
        }
        val manifest = ui.manifest
        if (manifest != null && ui.phase != Phase.UpToDate) {
            Text(
                "Available update: LCL ${ProductVersion.shown(manifest.productVersion.toString())} · released ${manifest.publishedAt.take(10)} · ${size(manifest.android.size)}",
                Modifier.testTag("update_available"),
            )
            // Release notes are text to read, never interpreted.
            Text(
                manifest.releaseNotes,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("update_notes"),
            )
        }
        ui.progress?.let { (done, total) ->
            LinearProgressIndicator(
                progress = { if (total > 0) done.toFloat() / total else 0f },
                modifier = Modifier.fillMaxWidth().testTag("update_progress"),
            )
        }
        val busy = ui.phase in setOf(Phase.Checking, Phase.Downloading, Phase.Installing)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(
                onClick = updates::check,
                enabled = !busy,
                modifier = Modifier.testTag("update_check"),
            ) {
                Text("Check for updates")
            }
            if (manifest != null && (ui.phase == Phase.Available || ui.phase == Phase.Failed)) {
                Button(onClick = updates::update, modifier = Modifier.testTag("update_now")) {
                    Text("Update")
                }
            }
            if (ui.phase == Phase.NeedsPermission) {
                Button(
                    onClick = updates::askPermission,
                    modifier = Modifier.testTag("update_allow"),
                ) {
                    Text("Allow installing")
                }
            }
        }
        if (ui.phase == Phase.NeedsPermission) {
            Text(
                "Android asks you once to allow LCL to install apps (Install unknown apps). The verified update then continues.",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Text(
            "LCL looks for new releases on the official LCL GitHub page at most once a day, and when you press Check. " +
                "An update is offered only after its signature is verified, installed only after the APK matches that " +
                "signed release, and Android keeps your paired PCs and settings. You do not need to be connected to a PC.",
            style = MaterialTheme.typography.bodySmall,
        )
    }
}
