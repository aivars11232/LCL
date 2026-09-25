package io.lcl.workspace.ui

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.lcl.workspace.AppContainer
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.remote.PairingLink
import io.lcl.workspace.workspace.LclNames
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collectLatest

sealed interface Screen {
    data object Home : Screen
    data class Pair(val link: String? = null) : Screen
    data object Workspace : Screen
    data object Settings : Screen
    data object About : Screen
    data class LocalDocument(val uri: Uri) : Screen
}

@Composable
fun LclRoot(container: AppContainer, incoming: MutableStateFlow<Intent?>) {
    val settings by container.settings.collectAsState()
    val connection by container.connection.state.collectAsState()
    var screen by remember {
        mutableStateOf<Screen>(if (container.pcs.activeId() != null) Screen.Workspace else Screen.Home)
    }
    val snackbar = remember { SnackbarHostState() }
    val intent by incoming.collectAsState()

    LaunchedEffect(intent) {
        val opened = intent ?: return@LaunchedEffect
        incoming.value = null
        val data = opened.data ?: return@LaunchedEffect
        when {
            data.scheme == PairingLink.SCHEME -> screen = Screen.Pair(data.toString())
            opened.action == Intent.ACTION_VIEW -> {
                val name = data.lastPathSegment?.substringAfterLast('/') ?: ""
                if (LclNames.isDocument(name) || data.scheme == "content") screen = Screen.LocalDocument(data)
            }
        }
    }
    LaunchedEffect(Unit) {
        // The latest message replaces the one showing: old news never queues up.
        container.workspace.messages.collectLatest { snackbar.showSnackbar(it) }
    }

    LclTheme(settings.theme) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Box(Modifier.fillMaxSize().safeDrawingPadding()) {
                Column(Modifier.fillMaxSize()) {
                    ConnectionBanner(connection, onReconnect = container.connection::reconnectNow, onOpenPcs = { screen = Screen.Home })
                    val back = { screen = if (container.pcs.activeId() != null && screen != Screen.Workspace) Screen.Workspace else Screen.Home }
                    BackHandler(enabled = screen != Screen.Home && screen != Screen.Workspace) { back() }
                    when (val current = screen) {
                        Screen.Home -> HomeScreen(
                            container = container,
                            state = connection,
                            onPair = { screen = Screen.Pair() },
                            onOpenWorkspace = { screen = Screen.Workspace },
                            onSettings = { screen = Screen.Settings },
                            onAbout = { screen = Screen.About },
                        )
                        is Screen.Pair -> PairScreen(
                            container = container,
                            initialLink = current.link,
                            onPaired = { screen = Screen.Workspace },
                            onBack = back,
                        )
                        Screen.Workspace -> WorkspaceScreen(
                            container = container,
                            connection = connection,
                            settings = settings,
                            onHome = { screen = Screen.Home },
                        )
                        Screen.Settings -> SettingsScreen(settings, container::updateSettings, back)
                        Screen.About -> AboutScreen(container, connection, back)
                        is Screen.LocalDocument -> LocalDocumentScreen(container, current.uri, settings, back)
                    }
                }
                SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).padding(16.dp))
            }
        }
    }
}

/** One line of text for where the connection stands. */
fun describe(state: ConnectionState): String = when (state) {
    ConnectionState.NoPc -> "No PC paired"
    is ConnectionState.Connecting -> "Connecting to ${state.pc.name}…"
    is ConnectionState.Connected -> "Connected to ${state.pc.name}"
    is ConnectionState.Reconnecting -> "Reconnecting to ${state.pc.name}" +
        (if (state.retryInSeconds > 0) " in ${state.retryInSeconds} s" else "") +
        (if (state.reason.isNotBlank()) " — ${state.reason}" else "")
    is ConnectionState.Offline -> "Offline — no network. ${state.pc.name} stays paired."
    is ConnectionState.Disconnected -> "Disconnected from ${state.pc.name}. Still paired."
    is ConnectionState.Revoked -> state.message
    is ConnectionState.NotPaired -> state.message
}

@Composable
fun ConnectionBanner(state: ConnectionState, onReconnect: () -> Unit, onOpenPcs: () -> Unit) {
    val colors = LocalLclColors.current
    val (tint, label) = when (state) {
        is ConnectionState.Connected -> colors.good to "Connected"
        is ConnectionState.Connecting, is ConnectionState.Reconnecting -> colors.warn to "Reconnecting"
        is ConnectionState.Offline, is ConnectionState.Disconnected -> colors.symbol to "Offline"
        is ConnectionState.Revoked, is ConnectionState.NotPaired -> colors.bad to "Not trusted"
        ConnectionState.NoPc -> colors.symbol to "No PC"
    }
    Row(
        Modifier
            .fillMaxWidth()
            .background(tint.copy(alpha = 0.14f))
            .padding(horizontal = 12.dp, vertical = 4.dp)
            .testTag("connection"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("● ", color = tint)
        Text(label, fontWeight = FontWeight.SemiBold, color = tint, modifier = Modifier.testTag("connection_label"))
        Text("  " + describe(state), style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f), maxLines = 2)
        when (state) {
            is ConnectionState.Reconnecting, is ConnectionState.Disconnected, is ConnectionState.Offline ->
                TextButton(onClick = onReconnect) { Text("Reconnect") }
            is ConnectionState.Revoked, is ConnectionState.NotPaired, ConnectionState.NoPc ->
                TextButton(onClick = onOpenPcs) { Text("PCs") }
            else -> Unit
        }
    }
}

