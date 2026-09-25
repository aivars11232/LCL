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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
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
import io.lcl.workspace.workspace.LclNames
import io.lcl.workspace.workspace.OpenDocument
import io.lcl.workspace.workspace.RunGrants
import io.lcl.workspace.workspace.RunState
import io.lcl.workspace.workspace.WorkspaceController
import io.lcl.workspace.workspace.WorkspaceUi
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

@Composable
fun WorkspaceScreen(container: AppContainer, connection: ConnectionState, settings: AppSettings, onHome: () -> Unit) {
    val controller = container.workspace
    val ui by controller.ui.collectAsState()
    val editors = remember { mutableMapOf<String, EditorState>() }
    var showFiles by remember { mutableStateOf(ui.active == null) }
    val connected = connection is ConnectionState.Connected

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= 840.dp
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                FilesPane(controller, ui, connected, onHome, onOpened = {}, modifier = Modifier.width(300.dp).fillMaxHeight())
                Box(Modifier.width(1.dp).fillMaxHeight().background(MaterialTheme.colorScheme.outlineVariant))
                EditorPane(controller, ui, editors, settings, connected, onFiles = null, modifier = Modifier.weight(1f))
            }
        } else if (showFiles || ui.activeDocument == null) {
            FilesPane(controller, ui, connected, onHome, onOpened = { showFiles = false }, modifier = Modifier.fillMaxSize())
        } else {
            BackHandler { showFiles = true }
            EditorPane(controller, ui, editors, settings, connected, onFiles = { showFiles = true }, modifier = Modifier.fillMaxSize())
        }
    }
    ui.run?.paused?.let { pause -> ApprovalDialog(pause, controller) }
}

@Composable
private fun FilesPane(
    controller: WorkspaceController,
    ui: WorkspaceUi,
    connected: Boolean,
    onHome: () -> Unit,
    onOpened: () -> Unit,
    modifier: Modifier,
) {
    var picking by remember { mutableStateOf(false) }
    var creating by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    Column(modifier.padding(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onHome, Modifier.testTag("home")) { Text("PCs") }
            Box(Modifier.weight(1f)) {
                TextButton(onClick = { picking = true }, enabled = ui.projects.size > 1) {
                    Text(ui.project?.name ?: "No project", fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                DropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                    ui.projects.forEach { p ->
                        DropdownMenuItem(
                            text = { Text(p.name + if (p.isDefault) "  (default)" else "") },
                            onClick = { picking = false; controller.selectProject(p.id) },
                        )
                    }
                }
            }
            TextButton(onClick = { scope.launch { controller.refreshTree() } }, enabled = connected) { Text("Refresh") }
            TextButton(onClick = { creating = true }, enabled = connected && ui.project != null, modifier = Modifier.testTag("new_document")) { Text("New") }
        }
        ui.project?.let {
            Text(it.root, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        HorizontalDivider(Modifier.padding(vertical = 4.dp))
        if (ui.tree.isEmpty()) {
            Text(
                if (connected) "No LCL documents here yet. New creates one." else "Connect to the PC to see its projects.",
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(8.dp),
            )
        }
        LazyColumn(Modifier.fillMaxSize()) {
            items(ui.tree, key = { it.id }) { entry ->
                val depth = entry.id.count { it == '/' }
                val name = entry.id.substringAfterLast('/')
                val open = ui.documents.firstOrNull { it.project == ui.project?.id && it.id == entry.id }
                Row(
                    Modifier
                        .fillMaxWidth()
                        .then(if (entry.directory) Modifier else Modifier.clickable { controller.open(entry.id); onOpened() })
                        .padding(start = (8 + depth * 14).dp, top = 10.dp, bottom = 10.dp, end = 8.dp)
                        .testTag("file:${entry.id}"),
                ) {
                    if (open?.dirty == true) Text("● ", color = LocalLclColors.current.warn)
                    Text(
                        if (entry.directory) "$name/" else name,
                        color = if (entry.directory) MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f) else MaterialTheme.colorScheme.onSurface,
                        fontWeight = if (open != null) FontWeight.SemiBold else null,
                    )
                }
            }
        }
    }
    if (creating) NewDocumentDialog(ui.defaultEnding, onCreate = { controller.create(it); creating = false; onOpened() }, onDismiss = { creating = false })
}

@Composable
private fun NewDocumentDialog(ending: String, onCreate: (String) -> Unit, onDismiss: () -> Unit) {
    var name by remember { mutableStateOf("untitled$ending") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New document") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("A path inside the project. A name without an ending is created as $ending, the PC's default. Type .lcl or .lcl.txt yourself to choose either; the ending you type is kept.")
                OutlinedTextField(value = name, onValueChange = { name = it }, singleLine = true, modifier = Modifier.testTag("new_name"))
                Text("Will be created as ${LclNames.defaultName(name, ending)}", style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = { TextButton(onClick = { if (name.isNotBlank()) onCreate(name) }, Modifier.testTag("create")) { Text("Create") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
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
    var runOptions by remember { mutableStateOf(false) }
    Column(modifier) {
        // Tabs.
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            if (onFiles != null) TextButton(onClick = onFiles, Modifier.testTag("files")) { Text("Files") }
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
        // Actions.
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            val edit = connected && doc.conflict == null
            // Phone first: the actions used most come first and fit a phone's width.
            val compact = PaddingValues(horizontal = 12.dp, vertical = 6.dp)
            OutlinedButton(onClick = { controller.save(key) }, enabled = connected && doc.dirty, contentPadding = compact, modifier = Modifier.testTag("action_save")) { Text("Save") }
            OutlinedButton(onClick = { controller.analyse("check"); panel = Panel.DIAGNOSTICS }, enabled = connected, contentPadding = compact, modifier = Modifier.testTag("action_check")) { Text("Check") }
            OutlinedButton(onClick = { runOptions = true }, enabled = connected && ui.run?.finished != false, contentPadding = compact, modifier = Modifier.testTag("action_run")) { Text("Run") }
            OutlinedButton(onClick = { controller.analyse("inspect"); panel = Panel.STRUCTURE }, enabled = connected, contentPadding = compact, modifier = Modifier.testTag("action_inspect")) { Text("Inspect") }
            OutlinedButton(onClick = { controller.analyse("validate"); panel = Panel.DIAGNOSTICS }, enabled = connected, contentPadding = compact, modifier = Modifier.testTag("action_validate")) { Text("Validate") }
            OutlinedButton(onClick = { if (doc.dirty) confirmReload = true else controller.reload(key) }, enabled = connected, contentPadding = compact, modifier = Modifier.testTag("action_reload")) { Text("Reload") }
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
        if (!connected) Banner("Offline: this copy is read-only until the PC is back.", LocalLclColors.current.symbol) {}
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
            readOnly = !connected || doc.conflict != null,
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
    if (runOptions) {
        RunOptionsDialog(onRun = { grants -> runOptions = false; controller.run(grants); panel = Panel.RUN }, onDismiss = { runOptions = false })
    }
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
