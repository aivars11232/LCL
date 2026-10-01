package io.lcl.workspace.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.lcl.workspace.AppContainer
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.editor.EditorState
import io.lcl.workspace.workspace.ProjectInfo

@Composable
fun WorkspaceScreen(
    container: AppContainer,
    connection: ConnectionState,
    settings: AppSettings,
    onHome: () -> Unit,
    onManual: () -> Unit,
    // Held by the caller, so undo history and selections outlive a visit to
    // the Manual tab.
    editors: MutableMap<String, EditorState>,
) {
    val controller = container.workspace
    val ui by controller.ui.collectAsState()
    var showFiles by remember { mutableStateOf(ui.active == null) }
    val connected = connection is ConnectionState.Connected
    // A sync of a phone project, held here, above the layouts: the project
    // may leave the list (removed once the PC confirmed) and the layout may
    // change with it, and the dialog still shows the result until Close.
    var syncing by remember { mutableStateOf<ProjectInfo?>(null) }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= 840.dp
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                FilesPane(
                    controller,
                    ui,
                    connected,
                    onHome,
                    onManual,
                    onOpened = {},
                    onSync = { syncing = it },
                    modifier = Modifier.width(300.dp).fillMaxHeight(),
                )
                Box(
                    Modifier.width(1.dp)
                        .fillMaxHeight()
                        .background(MaterialTheme.colorScheme.outlineVariant)
                )
                EditorPane(
                    controller,
                    ui,
                    editors,
                    settings,
                    connected,
                    onFiles = null,
                    modifier = Modifier.weight(1f),
                )
            }
        } else if (showFiles || ui.activeDocument == null) {
            FilesPane(
                controller,
                ui,
                connected,
                onHome,
                onManual,
                onOpened = { showFiles = false },
                onSync = { syncing = it },
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            val drawer = rememberDrawerState(DrawerValue.Closed)
            // With the tree closed, Back shows the files full screen, as before.
            BackHandler(enabled = !drawer.isOpen) { showFiles = true }
            // Opening the tree lists the project again, so it shows what is on the PC now.
            LaunchedEffect(drawer.targetValue) {
                if (drawer.targetValue == DrawerValue.Open && connected) controller.refreshTree()
            }
            FilesDrawer(
                drawer,
                width = minOf(maxWidth * 0.86f, 360.dp),
                files = { close ->
                    FilesPane(
                        controller,
                        ui,
                        connected,
                        onHome,
                        onManual,
                        onOpened = close,
                        onSync = { syncing = it },
                        modifier = Modifier.fillMaxSize(),
                    )
                },
            ) { open ->
                EditorPane(
                    controller,
                    ui,
                    editors,
                    settings,
                    connected,
                    onFiles = open,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }
    ui.run?.paused?.let { pause -> ApprovalDialog(pause, controller) }
    syncing?.let { SyncDialog(controller, ui, it, onDismiss = { syncing = null }) }
}
