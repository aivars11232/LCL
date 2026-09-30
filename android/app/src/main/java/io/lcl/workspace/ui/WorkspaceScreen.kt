package io.lcl.workspace.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DrawerState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.rememberDrawerState
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import io.lcl.workspace.workspace.ScaffoldPreview
import kotlinx.coroutines.delay
import androidx.compose.foundation.layout.heightIn
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.lcl.workspace.AppContainer
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.editor.CodeEditor
import io.lcl.workspace.editor.EditorState
import io.lcl.workspace.editor.INDENT
import io.lcl.workspace.editor.Utf8Index
import io.lcl.workspace.remote.arr
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.str
import io.lcl.workspace.local.LocalProjects
import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.LclNames
import io.lcl.workspace.workspace.OpenDocument
import io.lcl.workspace.workspace.ProjectInfo
import io.lcl.workspace.workspace.Readiness
import io.lcl.workspace.workspace.RoleInfo
import io.lcl.workspace.workspace.RunGrants
import io.lcl.workspace.workspace.RunState
import io.lcl.workspace.workspace.SyncChoice
import io.lcl.workspace.workspace.SyncPlan
import io.lcl.workspace.workspace.SyncResult
import io.lcl.workspace.workspace.SyncState
import io.lcl.workspace.workspace.TreeRow
import io.lcl.workspace.workspace.WorkspaceController
import io.lcl.workspace.workspace.WorkspaceUi
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

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
                FilesPane(controller, ui, connected, onHome, onManual, onOpened = {}, onSync = { syncing = it }, modifier = Modifier.width(300.dp).fillMaxHeight())
                Box(Modifier.width(1.dp).fillMaxHeight().background(MaterialTheme.colorScheme.outlineVariant))
                EditorPane(controller, ui, editors, settings, connected, onFiles = null, modifier = Modifier.weight(1f))
            }
        } else if (showFiles || ui.activeDocument == null) {
            FilesPane(controller, ui, connected, onHome, onManual, onOpened = { showFiles = false }, onSync = { syncing = it }, modifier = Modifier.fillMaxSize())
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
                files = { close -> FilesPane(controller, ui, connected, onHome, onManual, onOpened = close, onSync = { syncing = it }, modifier = Modifier.fillMaxSize()) },
            ) { open ->
                EditorPane(controller, ui, editors, settings, connected, onFiles = open, modifier = Modifier.fillMaxSize())
            }
        }
    }
    ui.run?.paused?.let { pause -> ApprovalDialog(pause, controller) }
    syncing?.let { SyncDialog(controller, ui, it, onDismiss = { syncing = null }) }
}

/**
 * The project tree as a panel that slides in from the left over [content].
 * [content] is handed `open` for its Files button; [files] is handed `close`,
 * for choosing a file. Tapping outside the panel, swiping it back or Back also
 * close it. What the panel shows exists only while it is open, so nothing
 * hidden past the edge can be found or pressed; and while it is open it is
 * modal: what is behind it is hidden from accessibility services, as the scrim
 * hides it from touch.
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
        Box(if (showing) Modifier.clearAndSetSemantics {} else Modifier) { content { scope.launch { drawer.open() } } }
    }
}

/**
 * The project's folders and documents as the PC lists them, one folder at a
 * time: a folder unfolds (reading its children) and folds, a document opens,
 * and the document being edited is marked. Under a folder whose listing the
 * PC cut short, a note says so, for that folder alone.
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
    val faint = colors.onSurface.copy(alpha = 0.6f)
    LazyColumn(modifier) {
        items(rows, key = { it.entry.id }) { row ->
            val entry = row.entry
            if (row.note != null) {
                Text(
                    row.note,
                    style = MaterialTheme.typography.bodySmall,
                    color = LocalLclColors.current.warn,
                    modifier = Modifier
                        .padding(start = (4 + (row.depth + 1) * 14).dp, top = 6.dp, bottom = 6.dp, end = 8.dp)
                        .testTag("limited:${FileTree.parentOf(entry.id)}"),
                )
                return@items
            }
            val current = !entry.directory && entry.id == active
            Row(
                Modifier
                    .fillMaxWidth()
                    .background(if (current) colors.secondaryContainer else Color.Transparent)
                    .clickable { if (entry.directory) onToggle(entry.id) else onOpen(entry.id) }
                    .semantics {
                        if (entry.directory) stateDescription = if (row.folded) "Folded" else "Unfolded" else selected = current
                    }
                    .padding(start = (4 + row.depth * 14).dp, top = 10.dp, bottom = 10.dp, end = 8.dp)
                    .testTag("file:${entry.id}"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.width(18.dp)) { if (entry.directory) Text(if (row.folded) "▸" else "▾", color = faint) }
                if (entry.id in dirty) Text("● ", color = LocalLclColors.current.warn)
                Text(
                    if (entry.directory) "${row.name}/" else row.name,
                    color = when {
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
                    Text(
                        WorkspaceController.roleLabel(kind),
                        style = MaterialTheme.typography.labelSmall,
                        color = if (kind == "kind.project") colors.primary else faint,
                        modifier = Modifier.padding(horizontal = 6.dp).testTag("role:${entry.id}"),
                    )
                }
                if (entry.kind == "kind.project") {
                    TextButton(
                        onClick = { onReadiness(entry.id) },
                        enabled = connected,
                        contentPadding = PaddingValues(horizontal = 6.dp, vertical = 0.dp),
                        modifier = Modifier.testTag("readiness:${entry.id}"),
                    ) { Text("Readiness") }
                }
            }
        }
    }
}

@Composable
private fun FilesPane(
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
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onHome, Modifier.testTag("home")) { Text("Home") }
            Box {
                TextButton(onClick = { picking = true }, modifier = Modifier.testTag("project_picker")) {
                    Text(project?.name ?: "No project", fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                DropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                    val local = ui.projects.filter { it.local }
                    val onPc = ui.projects.filter { !it.local }
                    Text("On this phone", style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp))
                    local.forEach { p ->
                        DropdownMenuItem(text = { Text(p.name) }, onClick = { picking = false; controller.selectProject(p.id) }, modifier = Modifier.testTag("pick:${p.id}"))
                    }
                    DropdownMenuItem(text = { Text("New project on this phone…") }, onClick = { picking = false; makingLocal = true }, modifier = Modifier.testTag("pick_new_local"))
                    Text(if (connected) "On the PC" else "On the PC (not connected)", style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp))
                    onPc.forEach { p ->
                        DropdownMenuItem(
                            text = { Text(p.name + if (p.isDefault) "  (default)" else "") },
                            onClick = { picking = false; controller.selectProject(p.id) },
                            modifier = Modifier.testTag("pick:${p.id}"),
                        )
                    }
                }
            }
            TextButton(onClick = { scope.launch { controller.refreshTree() } }, enabled = usable) { Text("Refresh") }
            TextButton(onClick = { creating = true }, enabled = usable, modifier = Modifier.testTag("new_document")) { Text("New") }
            TextButton(onClick = { makingFolder = true }, enabled = usable, modifier = Modifier.testTag("new_folder")) { Text("Folder") }
            if (project?.local == true) {
                TextButton(onClick = { onSync(project) }, enabled = connected, modifier = Modifier.testTag("sync")) { Text("Sync…") }
                TextButton(onClick = { removing = true }, modifier = Modifier.testTag("remove_local")) { Text("Remove") }
            }
            ManualIcon(onManual)
        }
        project?.let {
            Text(
                if (it.local) "On this phone · Check, Validate, Inspect and Run need a PC" else it.root,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag("project_where"),
            )
        }
        project?.takeIf { it.local }?.let { local ->
            // Read again after an edit or save, a change in the explorer, or a sync.
            val unsaved = ui.documents.filter { it.project == local.id && it.dirty }.map { it.id }
            var status by remember(local.id) { mutableStateOf("") }
            LaunchedEffect(local.id, unsaved, ui.explorers[local.id], ui.syncEpoch) { status = controller.localStatus(local.id) }
            Text(status, style = MaterialTheme.typography.labelSmall, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.testTag("local_status"))
        }
        HorizontalDivider(Modifier.padding(vertical = 4.dp))
        if (ui.explorer.folders[""]?.entries.isNullOrEmpty()) {
            Text(
                when {
                    usable -> "Nothing here yet. New creates a document; Folder makes a folder."
                    project == null && ui.projects.none { it.local } -> "Connect to the PC to see its projects, or make a project on this phone from the project name above."
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
            onOpen = { id -> controller.open(id); onOpened() },
            onReadiness = controller::readiness,
            modifier = Modifier.fillMaxSize(),
        )
    }
    if (makingLocal) {
        NewLocalProjectDialog(onCreate = { name -> controller.createLocalProject(name); makingLocal = false }, onDismiss = { makingLocal = false })
    }
    if (removing && project != null) {
        // The PC is asked now whether it still holds everything as recorded; a record alone proves the past.
        var verdict by remember(project.id) { mutableStateOf<Result<Unit>?>(null) }
        LaunchedEffect(project.id) { verdict = controller.verifyRemovable(project.id) }
        AlertDialog(
            onDismissRequest = { removing = false },
            title = { Text("Remove ${project.name} from this phone?") },
            text = {
                Text(
                    verdict?.fold(
                        { "The PC confirmed just now that it holds every folder and document of it as they are here. The phone's copy is removed; the PC's stays." },
                        { it.message ?: "It stays on this phone." },
                    ) ?: "Asking the PC whether it still holds every folder and document as they are here…",
                    modifier = Modifier.testTag("remove_verdict"),
                )
            },
            confirmButton = {
                TextButton(onClick = { controller.removeLocalProject(project.id); removing = false }, enabled = verdict?.isSuccess == true, modifier = Modifier.testTag("confirm_remove_local")) { Text("Remove") }
            },
            dismissButton = { TextButton(onClick = { removing = false }) { Text("Cancel") } },
        )
    }
    if (makingFolder) {
        NewFolderDialog(
            inside = ui.activeDocument?.takeIf { it.project == ui.project?.id }?.let { FileTree.parentOf(it.id) } ?: "",
            onCreate = { folder -> controller.createFolder(folder); makingFolder = false },
            onDismiss = { makingFolder = false },
        )
    }
    if (creating) {
        NewDocumentDialog(
            ui.defaultEnding,
            ui.roles,
            preview = controller::previewScaffold,
            onCreate = { name, role, mode, digest -> controller.create(name, role, mode, digest); creating = false; onOpened() },
            onDismiss = { creating = false },
        )
    }
    ui.readiness?.let { readiness ->
        ReadinessDialog(readiness, onOpen = { unit -> controller.dismissReadiness(); controller.open(unit); onOpened() }, onDismiss = controller::dismissReadiness)
    }
}

/** New folder: a path inside the project, under a folder that exists; it can stay empty. */
@Composable
private fun NewFolderDialog(inside: String, onCreate: (String) -> Unit, onDismiss: () -> Unit) {
    var name by remember { mutableStateOf(if (inside.isEmpty()) "" else "$inside/") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New folder") },
        text = {
            Column {
                Text("A folder path inside the project. Its parent must exist; the folder can stay empty.", style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(value = name, onValueChange = { name = it }, singleLine = true, modifier = Modifier.testTag("new_folder_name"))
            }
        },
        confirmButton = {
            TextButton(onClick = { onCreate(name.trim().trimEnd('/')) }, enabled = name.trim().trimEnd('/').isNotEmpty(), modifier = Modifier.testTag("create_folder")) { Text("Create") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** The small help affordance: it switches to the Manual tab. */
@Composable
fun ManualIcon(onManual: () -> Unit) {
    TextButton(
        onClick = onManual,
        modifier = Modifier.testTag("manual_icon").semantics { contentDescription = "Users Manual" },
    ) { Text("?", fontWeight = FontWeight.Bold) }
}

@Composable
private fun NewDocumentDialog(
    ending: String,
    roles: List<RoleInfo>,
    preview: suspend (String, String, String) -> ScaffoldPreview?,
    onCreate: (String, String?, String, String?) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf("untitled$ending") }
    // null: a blank LCL file; otherwise a role the PC's engine defines.
    var role by remember { mutableStateOf<String?>(null) }
    var guided by remember { mutableStateOf(true) }
    var picking by remember { mutableStateOf(false) }
    // The PC's exact starting text for the current name, role and mode.
    var shown by remember { mutableStateOf<ScaffoldPreview?>(null) }
    val mode = if (guided) "guided" else "minimal"
    LaunchedEffect(name, role, mode) {
        shown = null
        val chosen = role ?: return@LaunchedEffect
        if (name.isBlank()) return@LaunchedEffect
        delay(250)
        shown = preview(name, chosen, mode)
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New document") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                Text("A path inside the project. A name without an ending is created as $ending, the PC's default. Type .lcl or .lcl.txt yourself to choose either; the ending you type is kept.")
                OutlinedTextField(value = name, onValueChange = { name = it }, singleLine = true, modifier = Modifier.testTag("new_name"))
                Text("Will be created as ${LclNames.defaultName(name, ending)}", style = MaterialTheme.typography.bodySmall)
                if (roles.isNotEmpty()) {
                    Box {
                        OutlinedButton(onClick = { picking = true }, modifier = Modifier.testTag("new_role")) {
                            Text("Kind: " + (roles.firstOrNull { it.role == role }?.label ?: "Blank LCL file"))
                        }
                        DropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                            DropdownMenuItem(text = { Text("Blank LCL file") }, onClick = { role = null; picking = false })
                            roles.forEach { r ->
                                DropdownMenuItem(
                                    text = { Text(r.label) },
                                    onClick = { role = r.role; picking = false },
                                    modifier = Modifier.testTag("role_choice:${r.role}"),
                                )
                            }
                        }
                    }
                    if (role != null) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            FilterChip(selected = guided, onClick = { guided = true }, label = { Text("Guided") }, modifier = Modifier.padding(end = 6.dp))
                            FilterChip(selected = !guided, onClick = { guided = false }, label = { Text("Minimal") })
                        }
                        Text(
                            "The PC writes exactly this starting text (its scaffold, or your default template on the PC). Fill the empty fields, then Check.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text(
                            shown?.text ?: "Asking the PC for the starting text…",
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier
                                .fillMaxWidth()
                                .heightIn(max = 180.dp)
                                .verticalScroll(rememberScrollState())
                                .testTag("new_preview"),
                        )
                    }
                }
            }
        },
        confirmButton = {
            // A file of a role is created only from the preview on screen.
            val ready = name.isNotBlank() && (role == null || shown?.let { it.role == role && it.mode == mode } == true)
            TextButton(
                onClick = { if (ready) onCreate(name, role, mode, shown?.digest) },
                enabled = ready,
                modifier = Modifier.testTag("create"),
            ) { Text("Create") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** A project's readiness, exactly as the PC's engine reported it. */
@Composable
private fun ReadinessDialog(readiness: Readiness, onOpen: (String) -> Unit, onDismiss: () -> Unit) {
    val colors = LocalLclColors.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("${readiness.entry} — ${readiness.status}", modifier = Modifier.testTag("readiness_status")) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (readiness.status != "ready") Text("Run is refused by the PC until the project is ready.", style = MaterialTheme.typography.bodySmall)
                readiness.files.forEach { file ->
                    Row(
                        Modifier.fillMaxWidth()
                            .then(if (file.unit != null) Modifier.clickable { onOpen(file.unit) } else Modifier)
                            .padding(vertical = 4.dp)
                            .testTag("readiness_file:${file.path}"),
                    ) {
                        Text(file.path, Modifier.weight(1f), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                        Text(
                            file.status,
                            color = when (file.status) { "ready" -> colors.good; "omitted" -> colors.symbol; else -> colors.bad },
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
                readiness.diagnostics.take(20).forEach { Text(it, color = colors.bad, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}

private enum class Panel { NONE, DIAGNOSTICS, STRUCTURE, RUN }

@Composable
private fun EditorPane(
    controller: WorkspaceController,
    ui: WorkspaceUi,
    editors: MutableMap<String, EditorState>,
    settings: AppSettings,
    connected: Boolean,
    onFiles: (() -> Unit)?,
    modifier: Modifier,
) {
    val doc = ui.activeDocument
    var panel by remember { mutableStateOf(Panel.NONE) }
    var confirmReload by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var runOptions by remember { mutableStateOf(false) }
    Column(modifier) {
        // Tabs.
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            if (onFiles != null) TextButton(onClick = onFiles, Modifier.testTag("files")) { Text("☰ Files") }
            ui.documents.forEach { d ->
                val key = WorkspaceUi.key(d)
                FilterChip(
                    selected = key == ui.active,
                    onClick = { controller.activate(key) },
                    label = { Text((if (d.dirty) "● " else "") + d.name, maxLines = 1) },
                    trailingIcon = { Text("✕", Modifier.clickable { controller.close(key) }.padding(2.dp)) },
                    modifier = Modifier.padding(end = 4.dp).testTag("tab:${d.id}"),
                )
            }
        }
        if (doc == null) {
            Text("No document open. Choose one from Files.", Modifier.padding(16.dp))
            return@Column
        }
        val key = WorkspaceUi.key(doc)
        val editor = editors.getOrPut(key) { EditorState() }
        // A document on this phone is edited and saved with no PC; the
        // engine's actions need one, and never run over a local copy.
        val local = LocalProjects.isLocal(doc.project)
        val editable = connected || local
        val engine = connected && !local
        // Actions.
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            val edit = editable && doc.conflict == null
            // Phone first: the actions used most come first and fit a phone's width.
            val compact = PaddingValues(horizontal = 12.dp, vertical = 6.dp)
            OutlinedButton(onClick = { controller.save(key) }, enabled = editable && doc.dirty, contentPadding = compact, modifier = Modifier.testTag("action_save")) { Text("Save") }
            OutlinedButton(onClick = { controller.analyse("check"); panel = Panel.DIAGNOSTICS }, enabled = engine, contentPadding = compact, modifier = Modifier.testTag("action_check")) { Text("Check") }
            OutlinedButton(onClick = { runOptions = true }, enabled = engine && ui.run?.finished != false, contentPadding = compact, modifier = Modifier.testTag("action_run")) { Text("Run") }
            OutlinedButton(onClick = { controller.analyse("inspect"); panel = Panel.STRUCTURE }, enabled = engine, contentPadding = compact, modifier = Modifier.testTag("action_inspect")) { Text("Inspect") }
            OutlinedButton(onClick = { controller.analyse("validate"); panel = Panel.DIAGNOSTICS }, enabled = engine, contentPadding = compact, modifier = Modifier.testTag("action_validate")) { Text("Validate") }
            OutlinedButton(onClick = { if (doc.dirty) confirmReload = true else controller.reload(key) }, enabled = editable, contentPadding = compact, modifier = Modifier.testTag("action_reload")) { Text("Reload") }
            if (local) OutlinedButton(onClick = { confirmDelete = true }, enabled = !doc.dirty, contentPadding = compact, modifier = Modifier.testTag("action_delete")) { Text("Delete") }
            TextButton(onClick = { editor.undo(doc.text)?.let { controller.edit(key, it) } }, enabled = edit && editor.history.canUndo) { Text("Undo") }
            TextButton(onClick = { editor.redo(doc.text)?.let { controller.edit(key, it) } }, enabled = edit && editor.history.canRedo) { Text("Redo") }
            TextButton(onClick = { controller.edit(key, editor.insert(doc.text, INDENT)) }, enabled = edit, modifier = Modifier.testTag("action_indent")) { Text("Indent") }
        }
        // Where the copy stands against the PC.
        doc.conflict?.let {
            Banner("${doc.name} changed on the PC since this copy was loaded. Nothing was overwritten.", LocalLclColors.current.warn) {
                TextButton(onClick = { controller.reload(key) }, Modifier.testTag("conflict_reload")) { Text("Use PC version") }
                TextButton(onClick = { controller.keepMine(key) }, Modifier.testTag("conflict_keep")) { Text("Keep mine") }
            }
        }
        if (doc.deletedOnPc) Banner("${doc.name} no longer exists on the PC.", LocalLclColors.current.bad) {}
        if (!connected && !local) Banner("Offline: this copy is read-only until the PC is back.", LocalLclColors.current.symbol) {}
        Row(Modifier.padding(horizontal = 12.dp)) {
            Text(
                (if (doc.dirty) "Unsaved" else "Saved") + " · ${position(doc.text, editor.selection)}",
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
                modifier = Modifier.testTag("doc_state"),
            )
        }
        CodeEditor(
            document = doc,
            state = editor,
            onTextChange = { controller.edit(key, it) },
            readOnly = !editable || doc.conflict != null,
            fontSize = settings.fontSize,
            lineNumbers = settings.lineNumbers,
            modifier = Modifier.weight(1f).fillMaxWidth(),
        )
        // Panels.
        Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            for ((p, label) in listOf(Panel.DIAGNOSTICS to "Diagnostics", Panel.STRUCTURE to "Structure", Panel.RUN to "Run")) {
                FilterChip(selected = panel == p, onClick = { panel = if (panel == p) Panel.NONE else p }, label = { Text(label) }, modifier = Modifier.testTag("panel_${label.lowercase()}"))
            }
        }
        if (panel != Panel.NONE) {
            Box(Modifier.fillMaxWidth().height(260.dp).background(MaterialTheme.colorScheme.surfaceVariant)) {
                when (panel) {
                    Panel.DIAGNOSTICS -> DiagnosticsPanel(ui.reports[key], doc) { byte ->
                        val at = Utf8Index(doc.text).charOf(byte)
                        editor.selection = TextRange(at)
                    }
                    Panel.STRUCTURE -> StructurePanel(ui.reports[key])
                    Panel.RUN -> RunPanel(ui.run, controller)
                    Panel.NONE -> Unit
                }
            }
        }
    }
    if (confirmReload) {
        AlertDialog(
            onDismissRequest = { confirmReload = false },
            title = { Text("Discard your edits?") },
            text = { Text("Reloading replaces this copy with the PC's file and throws away your unsaved edits.") },
            confirmButton = { TextButton(onClick = { confirmReload = false; doc?.let { controller.reload(WorkspaceUi.key(it)) } }) { Text("Reload") } },
            dismissButton = { TextButton(onClick = { confirmReload = false }) { Text("Cancel") } },
        )
    }
    if (confirmDelete && doc != null) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete ${doc.name} from this phone?") },
            text = { Text("The file is removed from this phone for good.") },
            confirmButton = { TextButton(onClick = { confirmDelete = false; controller.deleteLocal(WorkspaceUi.key(doc)) }, modifier = Modifier.testTag("confirm_delete")) { Text("Delete") } },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
    if (runOptions) {
        RunOptionsDialog(onRun = { grants -> runOptions = false; controller.run(grants); panel = Panel.RUN }, onDismiss = { runOptions = false })
    }
}

/**
 * Sync a project on this phone to a PC project: first a check of what the PC
 * holds at every destination, nothing written; then the person's choice for
 * each conflict; then the sync, each document confirmed by the PC; and, if
 * asked, the phone's copy removed only after every document is confirmed.
 */
@Composable
private fun SyncDialog(controller: WorkspaceController, ui: WorkspaceUi, project: ProjectInfo, onDismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    val pcProjects = ui.projects.filter { !it.local }
    var target by remember { mutableStateOf(pcProjects.firstOrNull()) }
    var picking by remember { mutableStateOf(false) }
    var folder by remember { mutableStateOf(project.name) }
    var plan by remember { mutableStateOf<SyncPlan?>(null) }
    var choices by remember { mutableStateOf(mapOf<String, SyncChoice>()) }
    var removeAfter by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var result by remember { mutableStateOf<SyncResult?>(null) }
    var busy by remember { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = { if (!busy) onDismiss() },
        title = {
            val done = result
            Text(
                when {
                    done == null -> "Sync ${project.name} to the PC"
                    done.removed -> "Synced and removed ${project.name}"
                    done.complete -> "Synced ${project.name}"
                    done.partial -> "Partial sync of ${project.name}"
                    else -> "Sync failed"
                },
            )
        },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val done = result
                val checked = plan
                when {
                    done != null -> {
                        done.outcomes.forEach { o ->
                            Text("${if (o.ok) "✓" else "✕"} ${o.id} → ${o.destination}: ${o.detail}", style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("sync_outcome:${o.id}"))
                        }
                        done.problem?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("sync_problem")) }
                        if (done.removed) Text("Removed from this phone.", fontWeight = FontWeight.SemiBold, modifier = Modifier.testTag("sync_removed"))
                        else if (done.complete) Text("Every folder and document is on the PC as it is here. Remove takes the phone's copy away once the PC confirms that again; the PC's stays.", style = MaterialTheme.typography.bodySmall)
                        else Text(
                            if (done.partial) "Partial sync: what the PC confirmed is marked ✓, the rest is not on the PC. The phone's copy is unchanged; syncing again continues safely."
                            else "The phone's copy is unchanged. Nothing was removed.",
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("sync_kept"),
                        )
                    }
                    checked == null -> {
                        Text("Nothing is written until you press Sync. First the PC is asked what it holds at each destination.", style = MaterialTheme.typography.bodySmall)
                        Box {
                            OutlinedButton(onClick = { picking = true }, modifier = Modifier.testTag("sync_pc_project")) { Text("PC project: ${target?.name ?: "none shared"}") }
                            DropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                                pcProjects.forEach { p -> DropdownMenuItem(text = { Text(p.name) }, onClick = { target = p; picking = false }) }
                            }
                        }
                        OutlinedTextField(value = folder, onValueChange = { folder = it }, singleLine = true, label = { Text("Folder in the PC project (empty for its root)") }, modifier = Modifier.testTag("sync_folder"))
                        problem?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("sync_problem")) }
                    }
                    else -> {
                        val absent = checked.items.count { it.state == SyncState.ABSENT }
                        val same = checked.items.count { it.state == SyncState.IDENTICAL }
                        val newer = checked.items.count { it.state == SyncState.SUPERSEDED }
                        Text(
                            "$absent to create, $same already on the PC, ${checked.conflicts.size} with different bytes on the PC." +
                                (if (newer > 0) " $newer to update with this phone's newer version." else "") +
                                (if (checked.folders.isNotEmpty()) " ${checked.folders.size} folder${if (checked.folders.size == 1) "" else "s"}." else ""),
                            modifier = Modifier.testTag("sync_summary"),
                        )
                        checked.conflicts.forEach { item ->
                            Column(Modifier.padding(vertical = 4.dp)) {
                                Text(item.destination, fontWeight = FontWeight.SemiBold, style = MaterialTheme.typography.bodySmall)
                                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                    for ((choice, label) in listOf(SyncChoice.KEEP_PC to "Keep PC version", SyncChoice.REPLACE to "Replace with phone version", SyncChoice.RENAME to "Save as ${item.renamed.substringAfterLast('/')}")) {
                                        FilterChip(selected = choices[item.id] == choice, onClick = { choices = choices + (item.id to choice) }, label = { Text(label) }, modifier = Modifier.testTag("sync_choice:${item.id}:${choice.name}"))
                                    }
                                }
                            }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(checked = removeAfter, onCheckedChange = { removeAfter = it }, modifier = Modifier.testTag("sync_remove_after"))
                            Text("Remove from this phone after the PC confirms every folder and document", style = MaterialTheme.typography.bodySmall)
                        }
                        problem?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("sync_problem")) }
                    }
                }
            }
        },
        confirmButton = {
            val done = result
            val checked = plan
            when {
                done != null -> TextButton(onClick = onDismiss) { Text("Close") }
                checked == null -> TextButton(
                    onClick = {
                        val pcProject = target ?: return@TextButton
                        busy = true; problem = null
                        scope.launch {
                            controller.planSync(project, pcProject, folder).fold({ plan = it }, { problem = it.message })
                            busy = false
                        }
                    },
                    enabled = !busy && target != null,
                    modifier = Modifier.testTag("sync_check"),
                ) { Text("Check") }
                else -> TextButton(
                    onClick = {
                        busy = true; problem = null
                        scope.launch {
                            result = controller.runSync(checked, choices, removeAfter)
                            busy = false
                        }
                    },
                    enabled = !busy && checked.conflicts.all { it.id in choices },
                    modifier = Modifier.testTag("sync_go"),
                ) { Text(if (removeAfter) "Sync and remove from phone" else "Sync") }
            }
        },
        dismissButton = { if (result == null) TextButton(onClick = onDismiss, enabled = !busy) { Text("Cancel") } },
    )
}

/** Line and column of the cursor, lines counted by line feeds, columns in characters. */
private fun position(text: String, selection: TextRange): String {
    val at = selection.start.coerceIn(0, text.length)
    val line = text.substring(0, at).count { it == '\n' } + 1
    val column = at - (text.lastIndexOf('\n', at - 1) + 1) + 1
    return "$line:$column"
}

@Composable
private fun Banner(message: String, tint: androidx.compose.ui.graphics.Color, actions: @Composable () -> Unit) {
    Row(
        Modifier.fillMaxWidth().background(tint.copy(alpha = 0.15f)).padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(message, style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f))
        actions()
    }
}

@Composable
private fun DiagnosticsPanel(report: Pair<String, JsonObject>?, doc: OpenDocument, onReveal: (Int) -> Unit) {
    val colors = LocalLclColors.current
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(8.dp)) {
        if (report == null) return Text("Nothing checked yet. Check, Validate or Inspect asks the PC's engine.")
        val (kind, json) = report
        val outcome = json.str("outcome") ?: "—"
        val accepted = outcome == "accepted"
        Text(
            "$kind: " + if (accepted) "accepted through ${json.str("reached")}" else "$outcome at ${json.str("reached")}",
            color = if (accepted) colors.good else colors.bad,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.testTag("outcome"),
        )
        if (accepted) Text("No stage produced an unhandled diagnostic. This is not a claim that the document ran.", style = MaterialTheme.typography.bodySmall)
        val diagnostics = json.arr("diagnostics").orEmpty().mapNotNull { it as? JsonObject }
        if (diagnostics.isEmpty()) Text("No diagnostics.", style = MaterialTheme.typography.bodySmall)
        for (d in diagnostics) {
            val here = d.str("source") == doc.id
            Column(
                Modifier
                    .fillMaxWidth()
                    .padding(vertical = 4.dp)
                    .then(if (here) Modifier.clickable { onReveal(d.obj("span")?.long("start")?.toInt() ?: 0) } else Modifier)
                    .testTag("diagnostic"),
            ) {
                Text(d.str("id") ?: "", fontFamily = FontFamily.Monospace, fontWeight = FontWeight.SemiBold)
                val position = d.obj("position")
                Text(
                    "${d.str("source")}:${position?.long("line")}:${position?.long("column")} · ${d.str("stage")} · ${d.str("default_status")}",
                    style = MaterialTheme.typography.bodySmall,
                    fontFamily = FontFamily.Monospace,
                )
                d.str("meaning")?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                d.str("detail")?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            }
        }
    }
}

@Composable
private fun StructurePanel(report: Pair<String, JsonObject>?) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(8.dp)) {
        val structure = report?.second?.obj("structure")
        if (structure == null) return Text("Inspect shows the imports, declarations and execution plan the PC's engine derived.")
        val imports = structure.arr("imports").orEmpty().mapNotNull { it as? JsonObject }
        if (imports.isNotEmpty()) {
            Text("Imports", fontWeight = FontWeight.SemiBold)
            imports.forEach { Text("${it.str("kind")} ${it.str("id")} — ${it.str("outcome")}", fontFamily = FontFamily.Monospace) }
        }
        val declarations = report.second.obj("navigation")?.arr("declarations").orEmpty().mapNotNull { it as? JsonObject }
        if (declarations.isNotEmpty()) {
            Text("Declarations (${declarations.size})", fontWeight = FontWeight.SemiBold)
            declarations.forEach { Text("${it.str("block")}  ${it.str("id")}", fontFamily = FontFamily.Monospace) }
        }
        val plan = structure.arr("plan").orEmpty().mapNotNull { it as? JsonObject }
        Text("Execution plan (${plan.size} nodes)", fontWeight = FontWeight.SemiBold, modifier = Modifier.testTag("plan"))
        plan.forEach { node ->
            Text("${node.str("block")}  ${node.str("id") ?: ""}  ${node.str("operation") ?: ""}", fontFamily = FontFamily.Monospace)
        }
    }
}

@Composable
private fun RunPanel(run: RunState?, controller: WorkspaceController) {
    val colors = LocalLclColors.current
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(8.dp)) {
        if (run == null) return Text("Run asks the PC to run this document. It pauses before every effect for you to allow or deny it here.")
        val state = when {
            run.connectionLost -> "connection lost — the run continues on the PC and is followed again on reconnect"
            run.paused != null -> "paused · waiting for you"
            run.finished -> "finished"
            else -> "running"
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("${run.run}: $state", fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f).testTag("run_state"))
            if (!run.finished) TextButton(onClick = { controller.answer("cancel") }) { Text("Stop") } else TextButton(onClick = controller::dismissRun) { Text("Clear") }
        }
        run.failed?.let { Text(it, color = colors.bad) }
        for (event in run.events) {
            val verdict = event.data.str("permission") ?: event.data.str("outcome") ?: ""
            Text("${event.name}  ${event.data.str("operation") ?: ""}  $verdict", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        }
        val completion = run.report?.obj("completion")
        if (completion != null) {
            val status = completion.str("terminal_status") ?: ""
            Text(status, color = if (status == "status.succeeded") colors.good else colors.bad, fontWeight = FontWeight.Bold, modifier = Modifier.testTag("terminal_status"))
            completion.str("reason")?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            completion.arr("outputs").orEmpty().mapNotNull { it as? JsonObject }.forEach {
                Text("${it.str("id")} = ${(it["value"] as? JsonPrimitive)?.content ?: "—"}  (${it.str("publication")})", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
            }
        } else if (run.finished && run.report != null) {
            Text("The run did not reach completion; Diagnostics says where it stopped.")
        }
    }
}

/** Host permissions for one run. Nothing is granted unless named here. */
@Composable
private fun RunOptionsDialog(onRun: (RunGrants) -> Unit, onDismiss: () -> Unit) {
    var read by remember { mutableStateOf("") }
    var write by remember { mutableStateOf("") }
    var program by remember { mutableStateOf("") }
    var host by remember { mutableStateOf("") }
    var inputs by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Run on the PC") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    "The document runs on the PC, through its engine. An effect happens only if the document authorizes it, " +
                        "the PC is granted it below, and you allow it when the run pauses to ask. Paths are the PC's.",
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedTextField(read, { read = it }, label = { Text("Read paths, one per line") }, modifier = Modifier.fillMaxWidth().testTag("grant_read"))
                OutlinedTextField(write, { write = it }, label = { Text("Write paths, one per line") }, modifier = Modifier.fillMaxWidth().testTag("grant_write"))
                OutlinedTextField(program, { program = it }, label = { Text("Programs, one per line") }, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(host, { host = it }, label = { Text("Network hosts, one per line") }, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(inputs, { inputs = it }, label = { Text("Inputs, id=expression per line") }, modifier = Modifier.fillMaxWidth())
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    val lines = WorkspaceController::grantsFrom
                    onRun(RunGrants(lines(read), lines(write), lines(program), lines(host), lines(inputs)))
                },
                modifier = Modifier.testTag("run_start"),
            ) { Text("Run") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** The run is paused before an effect: show what the engine says it is, and ask. */
@Composable
private fun ApprovalDialog(pause: JsonObject, controller: WorkspaceController) {
    // One answer per pause: a second tap must not answer again.
    var sent by remember(pause) { mutableStateOf(false) }
    fun reply(answer: String) {
        if (sent) return
        sent = true
        controller.answer(answer)
    }
    AlertDialog(
        onDismissRequest = {},
        title = { Text(if (pause.str("kind") == "effect") "Allow this effect?" else "Run this operation?") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                @Composable
                fun line(k: String, v: String?) {
                    if (v != null) Text("$k: $v", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
                line("operation", pause.str("operation"))
                line("target", pause.str("target"))
                pause.arr("parameters").orEmpty().mapNotNull { it as? JsonObject }.forEach { line(it.str("name") ?: "", it.str("value")) }
                line("category", pause.str("category"))
                line("effects", pause.arr("possible_effects")?.joinToString { (it as? JsonPrimitive)?.content ?: "" })
                line("written at", "${pause.str("source")} byte ${pause.obj("span")?.long("start")}")
                pause.obj("authorization")?.let { auth ->
                    line("authorized", auth.str("operation"))
                    line("permitted by", auth.arr("permitted_by")?.joinToString { (it as? JsonPrimitive)?.content ?: "" })
                }
                Text(
                    "Allowing grants host permission for this one request. It does not change what the document authorizes.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = { TextButton(onClick = { reply("continue") }, enabled = !sent, modifier = Modifier.testTag("approve_allow")) { Text("Allow") } },
        dismissButton = {
            Row {
                TextButton(onClick = { reply("cancel") }, enabled = !sent, modifier = Modifier.testTag("approve_stop")) { Text("Stop run") }
                TextButton(onClick = { reply("deny") }, enabled = !sent, modifier = Modifier.testTag("approve_deny")) { Text("Deny") }
            }
        },
    )
}
