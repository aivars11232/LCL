package io.lcl.workspace.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.lcl.workspace.AppContainer
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.data.PcRecord
import java.text.DateFormat
import java.util.Date

/** The PCs this device is paired with, and the connection to the active one. */
@Composable
fun HomeScreen(
    container: AppContainer,
    state: ConnectionState,
    onPair: () -> Unit,
    onOpenWorkspace: () -> Unit,
    onSettings: () -> Unit,
    onAbout: () -> Unit,
) {
    var forgetting by remember { mutableStateOf<PcRecord?>(null) }
    val pcs = container.connection.pcs()
    val active = state.pcOrNull
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("LCL", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        Text(
            "Your PC's LCL on this screen. The PC keeps the projects, runs the engine and decides every " +
                "effect; this device edits, checks and asks.",
            style = MaterialTheme.typography.bodyMedium,
        )
        if (pcs.isEmpty()) {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("No PC is paired yet.", fontWeight = FontWeight.SemiBold)
                    Text("On the PC, run `lcl-remote pair` (or use Settings → Android devices in the LCL workspace), scan the QR code it shows with Pair a PC → Scan QR code, here in this app, then approve this device on the PC.")
                }
            }
        }
        for (pc in pcs) {
            val isActive = pc.pcId == active?.pcId
            Card(Modifier.fillMaxWidth().testTag("pc:${pc.pcId}")) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(pc.name, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                    Text(if (isActive) describe(state) else "Paired, not active", style = MaterialTheme.typography.bodySmall)
                    Text(
                        "Fingerprint ${pc.fingerprint.take(16)}… · paired ${DateFormat.getDateInstance().format(Date(pc.pairedAt * 1000))}" +
                            (pc.lastConnected?.let { " · last connected ${DateFormat.getDateTimeInstance().format(Date(it * 1000))}" } ?: ""),
                        style = MaterialTheme.typography.bodySmall,
                        fontFamily = FontFamily.Monospace,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (isActive && state is ConnectionState.Connected) {
                            Button(onClick = onOpenWorkspace, Modifier.testTag("open_workspace")) { Text("Open workspace") }
                            OutlinedButton(onClick = container.connection::disconnect, Modifier.testTag("disconnect")) { Text("Disconnect") }
                        } else if (isActive && state !is ConnectionState.Revoked && state !is ConnectionState.NotPaired) {
                            Button(onClick = container.connection::reconnectNow, Modifier.testTag("reconnect")) { Text("Reconnect") }
                        } else if (!isActive) {
                            Button(onClick = { container.connection.connect(pc.pcId); onOpenWorkspace() }) { Text("Use this PC") }
                        }
                        TextButton(onClick = { forgetting = pc }, Modifier.testTag("forget:${pc.pcId}")) { Text("Forget PC") }
                    }
                }
            }
        }
        Button(onClick = onPair, Modifier.fillMaxWidth().testTag("pair_new")) { Text("Pair a PC") }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onSettings, Modifier.testTag("home_settings")) { Text("Settings") }
            OutlinedButton(onClick = onAbout, Modifier.testTag("home_about")) { Text("About") }
        }
        Spacer(Modifier.height(24.dp))
    }
    forgetting?.let { pc ->
        AlertDialog(
            onDismissRequest = { forgetting = null },
            title = { Text("Forget ${pc.name}?") },
            text = {
                Text(
                    "This deletes this device's key for ${pc.name} and everything it knows about it. " +
                        "To use it again you will need a new QR code from the PC. Disconnect instead " +
                        "if you only want to stop the connection.",
                )
            },
            confirmButton = {
                TextButton(onClick = { container.connection.forget(pc.pcId); forgetting = null }, Modifier.testTag("forget_confirm")) {
                    Text("Forget")
                }
            },
            dismissButton = { TextButton(onClick = { forgetting = null }) { Text("Cancel") } },
        )
    }
}
