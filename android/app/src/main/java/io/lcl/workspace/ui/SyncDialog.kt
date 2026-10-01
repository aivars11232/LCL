package io.lcl.workspace.ui

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.lcl.workspace.workspace.ProjectInfo
import io.lcl.workspace.workspace.SyncChoice
import io.lcl.workspace.workspace.SyncPlan
import io.lcl.workspace.workspace.SyncResult
import io.lcl.workspace.workspace.SyncState
import io.lcl.workspace.workspace.WorkspaceController
import io.lcl.workspace.workspace.WorkspaceUi
import kotlinx.coroutines.launch

/**
 * Sync a project on this phone to a PC project: first a check of what the PC holds at every
 * destination, nothing written; then the person's choice for each conflict; then the sync, each
 * document confirmed by the PC; and, if asked, the phone's copy removed only after every document
 * is confirmed.
 */
@Composable
internal fun SyncDialog(
    controller: WorkspaceController,
    ui: WorkspaceUi,
    project: ProjectInfo,
    onDismiss: () -> Unit,
) {
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
                }
            )
        },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                val done = result
                val checked = plan
                when {
                    done != null -> {
                        done.outcomes.forEach { outcome ->
                            Text(
                                "${if (outcome.ok) "✓" else "✕"} ${outcome.id} → ${outcome.destination}: ${outcome.detail}",
                                style = MaterialTheme.typography.bodySmall,
                                modifier = Modifier.testTag("sync_outcome:${outcome.id}"),
                            )
                        }
                        done.problem?.let {
                            Text(
                                it,
                                color = MaterialTheme.colorScheme.error,
                                style = MaterialTheme.typography.bodySmall,
                                modifier = Modifier.testTag("sync_problem"),
                            )
                        }
                        if (done.removed)
                            Text(
                                "Removed from this phone.",
                                fontWeight = FontWeight.SemiBold,
                                modifier = Modifier.testTag("sync_removed"),
                            )
                        else if (done.complete)
                            Text(
                                "Every folder and document is on the PC as it is here. Remove " +
                                    "takes the phone's copy away once the PC confirms that " +
                                    "again; the PC's stays.",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        else
                            Text(
                                if (done.partial)
                                    "Partial sync: what the PC confirmed is marked ✓, the rest " +
                                        "is not on the PC. The phone's copy is unchanged; " +
                                        "syncing again continues safely."
                                else "The phone's copy is unchanged. Nothing was removed.",
                                style = MaterialTheme.typography.bodySmall,
                                modifier = Modifier.testTag("sync_kept"),
                            )
                    }
                    checked == null -> {
                        Text(
                            "Nothing is written until you press Sync. First the PC is asked what " +
                                "it holds at each destination.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Box {
                            OutlinedButton(
                                onClick = { picking = true },
                                modifier = Modifier.testTag("sync_pc_project"),
                            ) {
                                Text("PC project: ${target?.name ?: "none shared"}")
                            }
                            DropdownMenu(
                                expanded = picking,
                                onDismissRequest = { picking = false },
                            ) {
                                pcProjects.forEach { option ->
                                    DropdownMenuItem(
                                        text = { Text(option.name) },
                                        onClick = {
                                            target = option
                                            picking = false
                                        },
                                    )
                                }
                            }
                        }
                        OutlinedTextField(
                            value = folder,
                            onValueChange = { folder = it },
                            singleLine = true,
                            label = { Text("Folder in the PC project (empty for its root)") },
                            modifier = Modifier.testTag("sync_folder"),
                        )
                        problem?.let {
                            Text(
                                it,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.testTag("sync_problem"),
                            )
                        }
                    }
                    else -> {
                        val absent = checked.items.count { it.state == SyncState.ABSENT }
                        val same = checked.items.count { it.state == SyncState.IDENTICAL }
                        val newer = checked.items.count { it.state == SyncState.SUPERSEDED }
                        Text(
                            "$absent to create, $same already on the PC, " +
                                "${checked.conflicts.size} with different bytes on the PC." +
                                (if (newer > 0) " $newer to update with this phone's newer version."
                                else "") +
                                (if (checked.folders.isNotEmpty())
                                    " ${checked.folders.size} " +
                                        "folder${if (checked.folders.size == 1) "" else "s"}."
                                else ""),
                            modifier = Modifier.testTag("sync_summary"),
                        )
                        checked.conflicts.forEach { item ->
                            Column(Modifier.padding(vertical = 4.dp)) {
                                Text(
                                    item.destination,
                                    fontWeight = FontWeight.SemiBold,
                                    style = MaterialTheme.typography.bodySmall,
                                )
                                Row(
                                    Modifier.horizontalScroll(rememberScrollState()),
                                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                                ) {
                                    for ((choice, label) in
                                        listOf(
                                            SyncChoice.KEEP_PC to "Keep PC version",
                                            SyncChoice.REPLACE to "Replace with phone version",
                                            SyncChoice.RENAME to
                                                "Save as ${item.renamed.substringAfterLast('/')}",
                                        )) {
                                        FilterChip(
                                            selected = choices[item.id] == choice,
                                            onClick = { choices = choices + (item.id to choice) },
                                            label = { Text(label) },
                                            modifier =
                                                Modifier.testTag(
                                                    "sync_choice:${item.id}:${choice.name}"
                                                ),
                                        )
                                    }
                                }
                            }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(
                                checked = removeAfter,
                                onCheckedChange = { removeAfter = it },
                                modifier = Modifier.testTag("sync_remove_after"),
                            )
                            Text(
                                "Remove from this phone after the PC confirms every folder and " +
                                    "document",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                        problem?.let {
                            Text(
                                it,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.testTag("sync_problem"),
                            )
                        }
                    }
                }
            }
        },
        confirmButton = {
            val done = result
            val checked = plan
            when {
                done != null -> TextButton(onClick = onDismiss) { Text("Close") }
                checked == null ->
                    TextButton(
                        onClick = {
                            val pcProject = target ?: return@TextButton
                            busy = true
                            problem = null
                            scope.launch {
                                controller
                                    .planSync(project, pcProject, folder)
                                    .fold({ plan = it }, { problem = it.message })
                                busy = false
                            }
                        },
                        enabled = !busy && target != null,
                        modifier = Modifier.testTag("sync_check"),
                    ) {
                        Text("Check")
                    }
                else ->
                    TextButton(
                        onClick = {
                            busy = true
                            problem = null
                            scope.launch {
                                result = controller.runSync(checked, choices, removeAfter)
                                busy = false
                            }
                        },
                        enabled = !busy && checked.conflicts.all { it.id in choices },
                        modifier = Modifier.testTag("sync_go"),
                    ) {
                        Text(if (removeAfter) "Sync and remove from phone" else "Sync")
                    }
            }
        },
        dismissButton = {
            if (result == null) TextButton(onClick = onDismiss, enabled = !busy) { Text("Cancel") }
        },
    )
}
