package io.lcl.workspace.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DrawerState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.ProjectInfo
import io.lcl.workspace.workspace.TreeRow
import io.lcl.workspace.workspace.WorkspaceController
import io.lcl.workspace.workspace.WorkspaceUi
import kotlinx.coroutines.launch

/**
 * The project tree as a panel that slides in from the left over [content]. [content] is handed
 * `open` for its Files button; [files] is handed `close`, for choosing a file. Tapping outside the
 * panel, swiping it back or Back also close it. What the panel shows exists only while it is open,
 * so nothing hidden past the edge can be found or pressed; and while it is open it is modal: what
 * is behind it is hidden from accessibility services, as the scrim hides it from touch.
 */
@Composable
internal fun FilesDrawer(
    drawer: DrawerState,
    width: Dp,
    files: @Composable (close: () -> Unit) -> Unit,
    content: @Composable (open: () -> Unit) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val showing = drawer.isOpen || drawer.targetValue == DrawerValue.Open
    BackHandler(enabled = drawer.isOpen) { scope.launch { drawer.close() } }
    ModalNavigationDrawer(
        drawerState = drawer,
        // Swiping only closes it: a swipe from the edge would fight the
        // editor's own sideways scrolling.
        gesturesEnabled = drawer.isOpen,
        drawerContent = {
            ModalDrawerSheet(Modifier.width(width).testTag("files_drawer")) {
                if (showing) files { scope.launch { drawer.close() } }
            }
        },
    ) {
        Box(if (showing) Modifier.clearAndSetSemantics {} else Modifier) {
            content { scope.launch { drawer.open() } }
        }
    }
}

/**
 * The project's folders and documents as the PC lists them, one folder at a time: a folder unfolds
 * (reading its children) and folds, a document opens, and the document being edited is marked.
 * Under a folder whose listing the PC cut short, a note says so, for that folder alone.
 */
@Composable
internal fun ProjectTree(
    rows: List<TreeRow>,
    /** The document being edited, if it is in this project. */
    active: String?,
    /** The documents open in tabs, and those of them with unsaved edits. */
    open: Set<String>,
    dirty: Set<String>,
    connected: Boolean,
    onToggle: (String) -> Unit,
    onOpen: (String) -> Unit,
    onReadiness: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val lcl = LocalLclColors.current
    val faint = lcl.inkDim
    LazyColumn(modifier) {
        items(rows, key = { it.entry.id }) { row ->
            val entry = row.entry
            if (row.note != null) {
                Text(
                    row.note,
                    style = MaterialTheme.typography.bodySmall,
                    color = LocalLclColors.current.warn,
                    modifier =
                        Modifier.padding(
                                start = (4 + (row.depth + 1) * 14).dp,
                                top = 6.dp,
                                bottom = 6.dp,
                                end = 8.dp,
                            )
                            .testTag("limited:${FileTree.parentOf(entry.id)}"),
                )
                return@items
            }
            val current = !entry.directory && entry.id == active
            Row(
                Modifier.fillMaxWidth()
                    // The document being edited, as on the desktop: the accent's soft tint and a
                    // bar at the edge.
                    .background(if (current) lcl.accentSoft else Color.Transparent)
                    .drawBehind {
                        if (current) drawRect(colors.primary, size = Size(2.dp.toPx(), size.height))
                    }
                    .clickable { if (entry.directory) onToggle(entry.id) else onOpen(entry.id) }
                    .semantics {
                        if (entry.directory)
                            stateDescription = if (row.folded) "Folded" else "Unfolded"
                        else selected = current
                    }
                    .padding(
                        start = (4 + row.depth * 14).dp,
                        top = 10.dp,
                        bottom = 10.dp,
                        end = 8.dp,
                    )
                    .testTag("file:${entry.id}"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.width(18.dp)) {
                    if (entry.directory) Text(if (row.folded) "▸" else "▾", color = faint)
                }
                if (entry.id in dirty) Text("● ", color = LocalLclColors.current.warn)
                Text(
                    if (entry.directory) "${row.name}/" else row.name,
                    color =
                        when {
                            current -> colors.onSecondaryContainer
                            entry.directory -> faint
                            else -> colors.onSurface
                        },
                    fontWeight = if (current || entry.id in open) FontWeight.SemiBold else null,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                // The role the file declares, as the PC's engine read it.
                entry.kind?.let { kind ->
                    val entryPoint = kind == "kind.project"
                    Text(
                        WorkspaceController.roleLabel(kind),
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        color = if (entryPoint) colors.primary else faint,
                        modifier =
                            Modifier.padding(horizontal = 6.dp)
                                .border(
                                    1.dp,
                                    if (entryPoint) colors.primary else lcl.line,
                                    CircleShape,
                                )
                                .padding(horizontal = 7.dp, vertical = 1.dp)
                                .testTag("role:${entry.id}"),
                    )
                }
                if (entry.kind == "kind.project") {
                    TextButton(
                        onClick = { onReadiness(entry.id) },
                        enabled = connected,
                        contentPadding = PaddingValues(horizontal = 6.dp, vertical = 0.dp),
                        modifier = Modifier.testTag("readiness:${entry.id}"),
                    ) {
                        Text("Readiness")
                    }
                }
            }
        }
    }
}

@Composable
internal fun FilesPane(
    controller: WorkspaceController,
    ui: WorkspaceUi,
    connected: Boolean,
    onHome: () -> Unit,
    onManual: () -> Unit,
    onOpened: () -> Unit,
    onSync: (ProjectInfo) -> Unit,
    modifier: Modifier,
) {
    var picking by remember { mutableStateOf(false) }
    var creating by remember { mutableStateOf(false) }
    var makingFolder by remember { mutableStateOf(false) }
    var makingLocal by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val project = ui.project
    // A project on this phone works with no PC; a PC's needs the connection.
    val usable = project != null && (connected || project.local)
    Column(modifier.padding(8.dp)) {
        // The project, with the way back to the dashboard and the manual beside it.
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            IconAction("⌂", "Home", onHome, Modifier.testTag("home"))
            Box(Modifier.weight(1f)) {
                OutlinedButton(
                    onClick = { picking = true },
                    contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                    modifier = Modifier.fillMaxWidth().testTag("project_picker"),
                ) {
                    Text(
                        project?.name ?: "No project",
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                DropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                    val local = ui.projects.filter { it.local }
                    val onPc = ui.projects.filter { !it.local }
                    Text(
                        "On this phone",
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    )
                    local.forEach { option ->
                        DropdownMenuItem(
                            text = { Text(option.name) },
                            onClick = {
                                picking = false
                                controller.selectProject(option.id)
                            },
                            modifier = Modifier.testTag("pick:${option.id}"),
                        )
                    }
                    DropdownMenuItem(
                        text = { Text("New project on this phone…") },
                        onClick = {
                            picking = false
                            makingLocal = true
                        },
                        modifier = Modifier.testTag("pick_new_local"),
                    )
                    Text(
                        if (connected) "On the PC" else "On the PC (not connected)",
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    )
                    onPc.forEach { option ->
                        DropdownMenuItem(
                            text = {
                                Text(option.name + if (option.isDefault) "  (default)" else "")
                            },
                            onClick = {
                                picking = false
                                controller.selectProject(option.id)
                            },
                            modifier = Modifier.testTag("pick:${option.id}"),
                        )
                    }
                }
            }
            ManualIcon(onManual)
        }
        project?.let {
            Text(
                if (it.local) "On this phone · Check, Validate, Inspect and Run need a PC"
                else it.root,
                style = MaterialTheme.typography.bodySmall,
                color = LocalLclColors.current.inkDim,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag("project_where"),
            )
        }
        project
            ?.takeIf { it.local }
            ?.let { local ->
                // Read again after an edit or save, a change in the explorer, or a sync.
                val unsaved =
                    ui.documents.filter { it.project == local.id && it.dirty }.map { it.id }
                var status by remember(local.id) { mutableStateOf("") }
                LaunchedEffect(local.id, unsaved, ui.explorers[local.id], ui.syncEpoch) {
                    status = controller.localStatus(local.id)
                }
                // Where the phone's copy stands against a PC, with what can be done about it.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        status,
                        style = MaterialTheme.typography.labelSmall,
                        color = LocalLclColors.current.inkDim,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).testTag("local_status"),
                    )
                    TextButton(
                        onClick = { onSync(local) },
                        enabled = connected,
                        modifier = Modifier.testTag("sync"),
                    ) {
                        Text("Sync…")
                    }
                    TextButton(
                        onClick = { removing = true },
                        modifier = Modifier.testTag("remove_local"),
                    ) {
                        Text("Remove")
                    }
                }
            }
        // The explorer's head, as the desktop's: a label, and its actions as small marks.
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            SectionLabel("Files", Modifier.weight(1f).padding(start = 4.dp))
            IconAction(
                "↻",
                "Refresh",
                { scope.launch { controller.refreshTree() } },
                enabled = usable,
            )
            IconAction(
                "+",
                "New document",
                { creating = true },
                Modifier.testTag("new_document"),
                enabled = usable,
            )
            IconAction(
                "▤",
                "New folder",
                { makingFolder = true },
                Modifier.testTag("new_folder"),
                enabled = usable,
            )
        }
        HorizontalDivider(color = LocalLclColors.current.line)
        if (ui.explorer.folders[""]?.entries.isNullOrEmpty()) {
            Text(
                when {
                    usable -> "Nothing here yet. New creates a document; Folder makes a folder."
                    project == null && ui.projects.none { it.local } ->
                        "Connect to the PC to see its projects, or make a project on this phone " +
                            "from the project name above."
                    else -> "Connect to the PC to see its projects."
                },
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(8.dp),
            )
        }
        val here = ui.documents.filter { it.project == ui.project?.id }
        ProjectTree(
            rows = FileTree.rows(ui.explorer),
            active = ui.activeDocument?.takeIf { it.project == ui.project?.id }?.id,
            open = here.map { it.id }.toSet(),
            dirty = here.filter { it.dirty }.map { it.id }.toSet(),
            connected = connected,
            onToggle = controller::toggleFolder,
            onOpen = { id ->
                controller.open(id)
                onOpened()
            },
            onReadiness = controller::readiness,
            modifier = Modifier.fillMaxSize(),
        )
    }
    if (makingLocal) {
        NewLocalProjectDialog(
            onCreate = { name ->
                controller.createLocalProject(name)
                makingLocal = false
            },
            onDismiss = { makingLocal = false },
        )
    }
    if (removing && project != null) {
        // The PC is asked now whether it still holds everything as recorded; a record alone proves
        // the past.
        var verdict by remember(project.id) { mutableStateOf<Result<Unit>?>(null) }
        LaunchedEffect(project.id) { verdict = controller.verifyRemovable(project.id) }
        AlertDialog(
            onDismissRequest = { removing = false },
            title = { Text("Remove ${project.name} from this phone?") },
            text = {
                Text(
                    verdict?.fold(
                        {
                            "The PC confirmed just now that it holds every folder and document " +
                                "of it as they are here. The phone's copy is removed; the PC's " +
                                "stays."
                        },
                        { it.message ?: "It stays on this phone." },
                    )
                        ?: "Asking the PC whether it still holds every folder and document as " +
                            "they are here…",
                    modifier = Modifier.testTag("remove_verdict"),
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        controller.removeLocalProject(project.id)
                        removing = false
                    },
                    enabled = verdict?.isSuccess == true,
                    modifier = Modifier.testTag("confirm_remove_local"),
                ) {
                    Text("Remove")
                }
            },
            dismissButton = { TextButton(onClick = { removing = false }) { Text("Cancel") } },
        )
    }
    if (makingFolder) {
        NewFolderDialog(
            inside =
                ui.activeDocument
                    ?.takeIf { it.project == ui.project?.id }
                    ?.let { FileTree.parentOf(it.id) } ?: "",
            onCreate = { folder ->
                controller.createFolder(folder)
                makingFolder = false
            },
            onDismiss = { makingFolder = false },
        )
    }
    if (creating) {
        NewDocumentDialog(
            ui.defaultEnding,
            ui.roles,
            preview = controller::previewScaffold,
            onCreate = { name, role, mode, digest ->
                controller.create(name, role, mode, digest)
                creating = false
                onOpened()
            },
            onDismiss = { creating = false },
        )
    }
    ui.readiness?.let { readiness ->
        ReadinessDialog(
            readiness,
            onOpen = { unit ->
                controller.dismissReadiness()
                controller.open(unit)
                onOpened()
            },
            onDismiss = controller::dismissReadiness,
        )
    }
}

/** The small help affordance: it switches to the Manual tab. */
@Composable
fun ManualIcon(onManual: () -> Unit) {
    TextButton(
        onClick = onManual,
        modifier =
            Modifier.testTag("manual_icon").semantics { contentDescription = "Users Manual" },
    ) {
        Text("?", fontWeight = FontWeight.Bold)
    }
}
