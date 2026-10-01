package io.lcl.workspace.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.editor.CodeEditor
import io.lcl.workspace.editor.EditorState
import io.lcl.workspace.editor.INDENT
import io.lcl.workspace.editor.Utf8Index
import io.lcl.workspace.local.LocalProjects
import io.lcl.workspace.workspace.WorkspaceController
import io.lcl.workspace.workspace.WorkspaceUi

private enum class Panel {
    NONE,
    DIAGNOSTICS,
    STRUCTURE,
    RUN,
}

@Composable
internal fun EditorPane(
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
        val lcl = LocalLclColors.current
        // Tabs, as the desktop's strip: sunk into the page, a hairline under
        // it, and the document shown marked by the accent under its name.
        Row(
            Modifier.fillMaxWidth()
                .background(lcl.sunken)
                .hairline(lcl.line, top = false)
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onFiles != null)
                TextButton(onClick = onFiles, Modifier.testTag("files")) { Text("☰ Files") }
            ui.documents.forEach { document ->
                val key = WorkspaceUi.key(document)
                LclTab(
                    selected = key == ui.active,
                    onClick = { controller.activate(key) },
                    modifier = Modifier.testTag("tab:${document.id}"),
                ) {
                    if (document.dirty) Text("● ", color = lcl.warn)
                    Text(document.name, maxLines = 1)
                    Text(
                        "✕",
                        Modifier.padding(start = 6.dp)
                            .clickable { controller.close(key) }
                            .padding(horizontal = 6.dp, vertical = 2.dp),
                        color = lcl.inkFaint,
                    )
                }
            }
        }
        if (doc == null) {
            Text(
                "No document open. Choose one from Files.",
                Modifier.padding(16.dp),
                color = lcl.inkDim,
            )
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
        Row(
            Modifier.fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 6.dp, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val edit = editable && doc.conflict == null
            // Phone first: the actions used most come first and fit a phone's width.
            val compact = PaddingValues(horizontal = 12.dp, vertical = 6.dp)
            OutlinedButton(
                onClick = { controller.save(key) },
                enabled = editable && doc.dirty,
                contentPadding = compact,
                modifier = Modifier.testTag("action_save"),
            ) {
                Text("Save")
            }
            OutlinedButton(
                onClick = {
                    controller.analyse("check")
                    panel = Panel.DIAGNOSTICS
                },
                enabled = engine,
                contentPadding = compact,
                modifier = Modifier.testTag("action_check"),
            ) {
                Text("Check")
            }
            OutlinedButton(
                onClick = { runOptions = true },
                enabled = engine && ui.run?.finished != false,
                contentPadding = compact,
                modifier = Modifier.testTag("action_run"),
            ) {
                Text("Run")
            }
            OutlinedButton(
                onClick = {
                    controller.analyse("inspect")
                    panel = Panel.STRUCTURE
                },
                enabled = engine,
                contentPadding = compact,
                modifier = Modifier.testTag("action_inspect"),
            ) {
                Text("Inspect")
            }
            OutlinedButton(
                onClick = {
                    controller.analyse("validate")
                    panel = Panel.DIAGNOSTICS
                },
                enabled = engine,
                contentPadding = compact,
                modifier = Modifier.testTag("action_validate"),
            ) {
                Text("Validate")
            }
            OutlinedButton(
                onClick = { if (doc.dirty) confirmReload = true else controller.reload(key) },
                enabled = editable,
                contentPadding = compact,
                modifier = Modifier.testTag("action_reload"),
            ) {
                Text("Reload")
            }
            if (local)
                OutlinedButton(
                    onClick = { confirmDelete = true },
                    enabled = !doc.dirty,
                    contentPadding = compact,
                    modifier = Modifier.testTag("action_delete"),
                ) {
                    Text("Delete")
                }
            TextButton(
                onClick = { editor.undo(doc.text)?.let { controller.edit(key, it) } },
                enabled = edit && editor.history.canUndo,
            ) {
                Text("Undo")
            }
            TextButton(
                onClick = { editor.redo(doc.text)?.let { controller.edit(key, it) } },
                enabled = edit && editor.history.canRedo,
            ) {
                Text("Redo")
            }
            TextButton(
                onClick = { controller.edit(key, editor.insert(doc.text, INDENT)) },
                enabled = edit,
                modifier = Modifier.testTag("action_indent"),
            ) {
                Text("Indent")
            }
        }
        // Where the copy stands against the PC.
        doc.conflict?.let {
            Banner(
                "${doc.name} changed on the PC since this copy was loaded. Nothing was " +
                    "overwritten.",
                LocalLclColors.current.warn,
            ) {
                TextButton(
                    onClick = { controller.reload(key) },
                    Modifier.testTag("conflict_reload"),
                ) {
                    Text("Use PC version")
                }
                TextButton(
                    onClick = { controller.keepMine(key) },
                    Modifier.testTag("conflict_keep"),
                ) {
                    Text("Keep mine")
                }
            }
        }
        if (doc.deletedOnPc)
            Banner("${doc.name} no longer exists on the PC.", LocalLclColors.current.bad) {}
        if (!connected && !local)
            Banner(
                "Offline: this copy is read-only until the PC is back.",
                LocalLclColors.current.symbol,
            ) {}
        // The status line, as the desktop's: monospace on a sunk strip.
        Row(
            Modifier.fillMaxWidth()
                .background(lcl.sunken)
                .padding(horizontal = 12.dp, vertical = 3.dp)
        ) {
            Text(
                (if (doc.dirty) "Unsaved" else "Saved") +
                    " · ${position(doc.text, editor.selection)}",
                style = MaterialTheme.typography.bodySmall,
                color = if (doc.dirty) lcl.warn else lcl.inkDim,
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
        // The inspector's tabs, as on the desktop.
        Row(Modifier.fillMaxWidth().background(lcl.sunken).hairline(lcl.line, top = true)) {
            for ((tab, label) in
                listOf(
                    Panel.DIAGNOSTICS to "Diagnostics",
                    Panel.STRUCTURE to "Structure",
                    Panel.RUN to "Run",
                )) {
                LclTab(
                    selected = panel == tab,
                    onClick = { panel = if (panel == tab) Panel.NONE else tab },
                    modifier = Modifier.weight(1f).testTag("panel_${label.lowercase()}"),
                ) {
                    Text(label)
                }
            }
        }
        if (panel != Panel.NONE) {
            Box(Modifier.fillMaxWidth().height(260.dp).background(lcl.sunken)) {
                when (panel) {
                    Panel.DIAGNOSTICS ->
                        DiagnosticsPanel(ui.reports[key], doc) { byte ->
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
            text = {
                Text(
                    "Reloading replaces this copy with the PC's file and throws away your " +
                        "unsaved edits."
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirmReload = false
                        doc?.let { controller.reload(WorkspaceUi.key(it)) }
                    }
                ) {
                    Text("Reload")
                }
            },
            dismissButton = { TextButton(onClick = { confirmReload = false }) { Text("Cancel") } },
        )
    }
    if (confirmDelete && doc != null) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete ${doc.name} from this phone?") },
            text = { Text("The file is removed from this phone for good.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirmDelete = false
                        controller.deleteLocal(WorkspaceUi.key(doc))
                    },
                    modifier = Modifier.testTag("confirm_delete"),
                ) {
                    Text("Delete")
                }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
    if (runOptions) {
        RunOptionsDialog(
            onRun = { grants ->
                runOptions = false
                controller.run(grants)
                panel = Panel.RUN
            },
            onDismiss = { runOptions = false },
        )
    }
}

/** Line and column of the cursor, lines counted by line feeds, columns in characters. */
internal fun position(text: String, selection: TextRange): String {
    val at = selection.start.coerceIn(0, text.length)
    val line = text.substring(0, at).count { it == '\n' } + 1
    val column = at - (text.lastIndexOf('\n', at - 1) + 1) + 1
    return "$line:$column"
}

@Composable
private fun Banner(
    message: String,
    tint: androidx.compose.ui.graphics.Color,
    actions: @Composable () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth()
            .background(tint.copy(alpha = 0.15f))
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(message, style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f))
        actions()
    }
}
