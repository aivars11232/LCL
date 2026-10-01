package io.lcl.workspace.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.lcl.workspace.remote.arr
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.str
import io.lcl.workspace.workspace.LclNames
import io.lcl.workspace.workspace.Readiness
import io.lcl.workspace.workspace.RoleInfo
import io.lcl.workspace.workspace.RunGrants
import io.lcl.workspace.workspace.ScaffoldPreview
import io.lcl.workspace.workspace.WorkspaceController
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** New folder: a path inside the project, under a folder that exists; it can stay empty. */
@Composable
internal fun NewFolderDialog(inside: String, onCreate: (String) -> Unit, onDismiss: () -> Unit) {
    var name by remember { mutableStateOf(if (inside.isEmpty()) "" else "$inside/") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New folder") },
        text = {
            Column {
                Text(
                    "A folder path inside the project. Its parent must exist; the folder can stay empty.",
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    singleLine = true,
                    modifier = Modifier.testTag("new_folder_name"),
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onCreate(name.trim().trimEnd('/')) },
                enabled = name.trim().trimEnd('/').isNotEmpty(),
                modifier = Modifier.testTag("create_folder"),
            ) {
                Text("Create")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
internal fun NewDocumentDialog(
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
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.verticalScroll(rememberScrollState()),
            ) {
                Text(
                    "A path inside the project. A name without an ending is created as $ending, the PC's default. Type .lcl or .lcl.txt yourself to choose either; the ending you type is kept."
                )
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    singleLine = true,
                    modifier = Modifier.testTag("new_name"),
                )
                Text(
                    "Will be created as ${LclNames.defaultName(name, ending)}",
                    style = MaterialTheme.typography.bodySmall,
                )
                if (roles.isNotEmpty()) {
                    Box {
                        OutlinedButton(
                            onClick = { picking = true },
                            modifier = Modifier.testTag("new_role"),
                        ) {
                            Text(
                                "Kind: " +
                                    (roles.firstOrNull { it.role == role }?.label
                                        ?: "Blank LCL file")
                            )
                        }
                        DropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                            DropdownMenuItem(
                                text = { Text("Blank LCL file") },
                                onClick = {
                                    role = null
                                    picking = false
                                },
                            )
                            roles.forEach { r ->
                                DropdownMenuItem(
                                    text = { Text(r.label) },
                                    onClick = {
                                        role = r.role
                                        picking = false
                                    },
                                    modifier = Modifier.testTag("role_choice:${r.role}"),
                                )
                            }
                        }
                    }
                    if (role != null) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            FilterChip(
                                selected = guided,
                                onClick = { guided = true },
                                label = { Text("Guided") },
                                modifier = Modifier.padding(end = 6.dp),
                            )
                            FilterChip(
                                selected = !guided,
                                onClick = { guided = false },
                                label = { Text("Minimal") },
                            )
                        }
                        Text(
                            "The PC writes exactly this starting text (its scaffold, or your default template on the PC). Fill the empty fields, then Check.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text(
                            shown?.text ?: "Asking the PC for the starting text…",
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.bodySmall,
                            modifier =
                                Modifier.fillMaxWidth()
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
            val ready =
                name.isNotBlank() &&
                    (role == null || shown?.let { it.role == role && it.mode == mode } == true)
            TextButton(
                onClick = { if (ready) onCreate(name, role, mode, shown?.digest) },
                enabled = ready,
                modifier = Modifier.testTag("create"),
            ) {
                Text("Create")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** A project's readiness, exactly as the PC's engine reported it. */
@Composable
internal fun ReadinessDialog(
    readiness: Readiness,
    onOpen: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val colors = LocalLclColors.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                "${readiness.entry} — ${readiness.status}",
                modifier = Modifier.testTag("readiness_status"),
            )
        },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (readiness.status != "ready")
                    Text(
                        "Run is refused by the PC until the project is ready.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                readiness.files.forEach { file ->
                    Row(
                        Modifier.fillMaxWidth()
                            .then(
                                if (file.unit != null) Modifier.clickable { onOpen(file.unit) }
                                else Modifier
                            )
                            .padding(vertical = 4.dp)
                            .testTag("readiness_file:${file.path}")
                    ) {
                        Text(
                            file.path,
                            Modifier.weight(1f),
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text(
                            file.status,
                            color =
                                when (file.status) {
                                    "ready" -> colors.good
                                    "omitted" -> colors.symbol
                                    else -> colors.bad
                                },
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
                readiness.diagnostics.take(20).forEach {
                    Text(it, color = colors.bad, style = MaterialTheme.typography.bodySmall)
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}

/** Host permissions for one run. Nothing is granted unless named here. */
@Composable
internal fun RunOptionsDialog(onRun: (RunGrants) -> Unit, onDismiss: () -> Unit) {
    var read by remember { mutableStateOf("") }
    var write by remember { mutableStateOf("") }
    var program by remember { mutableStateOf("") }
    var host by remember { mutableStateOf("") }
    var inputs by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Run on the PC") },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    "The document runs on the PC, through its engine. An effect happens only if the document authorizes it, " +
                        "the PC is granted it below, and you allow it when the run pauses to ask. Paths are the PC's.",
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedTextField(
                    read,
                    { read = it },
                    label = { Text("Read paths, one per line") },
                    modifier = Modifier.fillMaxWidth().testTag("grant_read"),
                )
                OutlinedTextField(
                    write,
                    { write = it },
                    label = { Text("Write paths, one per line") },
                    modifier = Modifier.fillMaxWidth().testTag("grant_write"),
                )
                OutlinedTextField(
                    program,
                    { program = it },
                    label = { Text("Programs, one per line") },
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    host,
                    { host = it },
                    label = { Text("Network hosts, one per line") },
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    inputs,
                    { inputs = it },
                    label = { Text("Inputs, id=expression per line") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    val lines = WorkspaceController::grantsFrom
                    onRun(
                        RunGrants(
                            lines(read),
                            lines(write),
                            lines(program),
                            lines(host),
                            lines(inputs),
                        )
                    )
                },
                modifier = Modifier.testTag("run_start"),
            ) {
                Text("Run")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** The run is paused before an effect: show what the engine says it is, and ask. */
@Composable
internal fun ApprovalDialog(pause: JsonObject, controller: WorkspaceController) {
    // One answer per pause: a second tap must not answer again.
    var sent by remember(pause) { mutableStateOf(false) }
    fun reply(answer: String) {
        if (sent) return
        sent = true
        controller.answer(answer)
    }
    AlertDialog(
        onDismissRequest = {},
        title = {
            Text(if (pause.str("kind") == "effect") "Allow this effect?" else "Run this operation?")
        },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                @Composable
                fun line(k: String, v: String?) {
                    if (v != null)
                        Text(
                            "$k: $v",
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.bodySmall,
                        )
                }
                line("operation", pause.str("operation"))
                line("target", pause.str("target"))
                pause
                    .arr("parameters")
                    .orEmpty()
                    .mapNotNull { it as? JsonObject }
                    .forEach { line(it.str("name") ?: "", it.str("value")) }
                line("category", pause.str("category"))
                line(
                    "effects",
                    pause.arr("possible_effects")?.joinToString {
                        (it as? JsonPrimitive)?.content ?: ""
                    },
                )
                line(
                    "written at",
                    "${pause.str("source")} byte ${pause.obj("span")?.long("start")}",
                )
                pause.obj("authorization")?.let { auth ->
                    line("authorized", auth.str("operation"))
                    line(
                        "permitted by",
                        auth.arr("permitted_by")?.joinToString {
                            (it as? JsonPrimitive)?.content ?: ""
                        },
                    )
                }
                Text(
                    "Allowing grants host permission for this one request. It does not change what the document authorizes.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { reply("continue") },
                enabled = !sent,
                modifier = Modifier.testTag("approve_allow"),
            ) {
                Text("Allow")
            }
        },
        dismissButton = {
            Row {
                TextButton(
                    onClick = { reply("cancel") },
                    enabled = !sent,
                    modifier = Modifier.testTag("approve_stop"),
                ) {
                    Text("Stop run")
                }
                TextButton(
                    onClick = { reply("deny") },
                    enabled = !sent,
                    modifier = Modifier.testTag("approve_deny"),
                ) {
                    Text("Deny")
                }
            }
        },
    )
}
