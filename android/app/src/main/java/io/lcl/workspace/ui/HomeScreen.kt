package io.lcl.workspace.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.ui.Alignment
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.lcl.workspace.AppContainer
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.data.PcRecord
import java.text.DateFormat
import java.util.Date

/**
 * Said while a PC may still trust an earlier key of this device, one that
 * pairing again could not yet retire (see ConnectionManager.pair): which
 * device it is on the PC, why it is not retired yet, and what to do.
 */
fun retiringNotice(pc: PcRecord): String {
    val devices = pc.retiring.joinToString(", ") { it.deviceId.ifBlank { "id unknown" } }
    val why = pc.retiring.mapNotNull { it.problem?.trimEnd('.') }.distinct().joinToString("; ")
    return "${pc.name} may still trust this device's earlier key (device $devices)" +
        (if (why.isEmpty()) "." else ": LCL could not end that yet — $why.") +
        " It tries again each time it connects to ${pc.name}. To end it now, revoke that device " +
        "on the PC: lcl-remote revoke ID, or Settings → Android devices in the LCL workspace."
}

/** The PCs this device is paired with, and the connection to the active one. */
@Composable
fun HomeScreen(
    container: AppContainer,
    state: ConnectionState,
    onPair: () -> Unit,
    onOpenWorkspace: () -> Unit,
    onSettings: () -> Unit,
    onAbout: () -> Unit,
    onUpdates: () -> Unit,
    onManual: () -> Unit,
) {
    val update by container.updates.ui.collectAsState()
    var forgetting by remember { mutableStateOf<PcRecord?>(null) }
    var makingLocal by remember { mutableStateOf(false) }
    val pcs by container.connection.records.collectAsState()
    val workspace by container.workspace.ui.collectAsState()
    val localProjects = workspace.projects.filter { it.local }
    val active = state.pcOrNull
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("LCL", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            ManualIcon(onManual)
        }
        Text(
            "Your PC's LCL on this screen: the PC keeps its projects, runs the engine and decides every " +
                "effect. Projects on this phone need no PC, and go to one only when you sync them.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // Projects on this phone: usable with no PC at all.
        Section("On this phone", Modifier.testTag("local_projects")) {
            if (localProjects.isEmpty()) {
                Text("No projects on this phone yet.", style = MaterialTheme.typography.bodySmall)
            }
            for (project in localProjects) {
                Row(Modifier.fillMaxWidth().listRow().padding(start = 12.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("▣", color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(end = 10.dp))
                    Text(project.name, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    TextButton(
                        onClick = { container.workspace.selectProject(project.id); onOpenWorkspace() },
                        modifier = Modifier.testTag("open_local:${project.name}"),
                    ) { Text("Open") }
                }
            }
            OutlinedButton(onClick = { makingLocal = true }, Modifier.testTag("new_local_project")) { Text("New project on this phone") }
        }
        if (pcs.isEmpty()) {
            Section("On a PC") {
                Text("No PC is paired yet.", fontWeight = FontWeight.SemiBold)
                Text(
                    "On the PC, run `lcl-remote pair` (or use Settings → Android devices in the LCL workspace), scan the QR code it shows with Pair a PC → Scan QR code, here in this app, then approve this device on the PC.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
        for (pc in pcs) {
            val isActive = pc.pcId == active?.pcId
            Section("On ${pc.name}", Modifier.testTag("pc:${pc.pcId}")) {
                Text(if (isActive) describe(state) else "Paired, not active", style = MaterialTheme.typography.bodySmall)
                Text(
                    "Fingerprint ${pc.fingerprint.take(16)}… · paired ${DateFormat.getDateInstance().format(Date(pc.pairedAt * 1000))}" +
                        (pc.lastConnected?.let { " · last connected ${DateFormat.getDateTimeInstance().format(Date(it * 1000))}" } ?: ""),
                    style = MaterialTheme.typography.bodySmall,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (pc.retiring.isNotEmpty()) {
                    Text(
                        retiringNotice(pc),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.testTag("retiring:${pc.pcId}"),
                    )
                }
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
        Button(onClick = onPair, Modifier.fillMaxWidth().testTag("pair_new")) { Text("Pair a PC") }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onSettings, Modifier.testTag("home_settings")) { Text("Settings") }
            OutlinedButton(onClick = onAbout, Modifier.testTag("home_about")) { Text("About") }
            // An update waiting is shown here, and nowhere does it interrupt.
            OutlinedButton(onClick = onUpdates, Modifier.testTag("home_updates")) {
                Text(if (update.phase == io.lcl.workspace.update.Phase.Available) "Update available" else "Updates")
            }
        }
        Spacer(Modifier.height(24.dp))
    }
    if (makingLocal) {
        NewLocalProjectDialog(
            onCreate = { name -> container.workspace.createLocalProject(name); makingLocal = false; onOpenWorkspace() },
            onDismiss = { makingLocal = false },
        )
    }
    forgetting?.let { pc ->
        AlertDialog(
            onDismissRequest = { forgetting = null },
            title = { Text("Forget ${pc.name}?") },
            text = {
                Text(
                    "This deletes this device's key for ${pc.name} and everything it knows about it. " +
                        "To use it again you will need a new QR code from the PC. Disconnect instead " +
                        "if you only want to stop the connection." +
                        if (pc.retiring.isEmpty()) {
                            ""
                        } else {
                            " ${pc.name} may also still trust an earlier key of this device (device " +
                                pc.retiring.joinToString(", ") { it.deviceId } + "): it asks the PC once more to " +
                                "stop trusting it, and is deleted too. If the PC cannot be reached, revoke that " +
                                "device on the PC."
                        },
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

/** One section of the dashboard: a small label, a rule, its rows. Compact, as a workspace tool, not a card deck. */
@Composable
private fun Section(label: String, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SectionLabel(label)
        HorizontalDivider(color = LocalLclColors.current.line)
        content()
    }
}

/** A new project on this phone: one folder under the app's own storage, named here. */
@Composable
fun NewLocalProjectDialog(onCreate: (String) -> Unit, onDismiss: () -> Unit) {
    var name by remember { mutableStateOf("") }
    val valid = name.isNotBlank() && !name.contains('/') && !name.trim().startsWith(".")
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New project on this phone") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("A project kept in this app's private storage. It needs no PC; Sync sends it to one when you choose.", style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(value = name, onValueChange = { name = it }, singleLine = true, label = { Text("Project name") }, modifier = Modifier.testTag("local_project_name"))
            }
        },
        confirmButton = { TextButton(onClick = { onCreate(name.trim()) }, enabled = valid, modifier = Modifier.testTag("create_local_project")) { Text("Create") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
