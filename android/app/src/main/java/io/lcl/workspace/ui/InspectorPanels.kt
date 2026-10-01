package io.lcl.workspace.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.lcl.workspace.remote.arr
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.str
import io.lcl.workspace.workspace.OpenDocument
import io.lcl.workspace.workspace.RunState
import io.lcl.workspace.workspace.WorkspaceController
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

@Composable
internal fun DiagnosticsPanel(
    report: Pair<String, JsonObject>?,
    doc: OpenDocument,
    onReveal: (Int) -> Unit,
) {
    val colors = LocalLclColors.current
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(8.dp)) {
        if (report == null)
            return Text("Nothing checked yet. Check, Validate or Inspect asks the PC's engine.")
        val (kind, json) = report
        val outcome = json.str("outcome") ?: "—"
        val accepted = outcome == "accepted"
        Text(
            "$kind: " +
                if (accepted) "accepted through ${json.str("reached")}"
                else "$outcome at ${json.str("reached")}",
            color = if (accepted) colors.good else colors.bad,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.testTag("outcome"),
        )
        if (accepted)
            Text(
                "No stage produced an unhandled diagnostic. This is not a claim that the " +
                    "document ran.",
                style = MaterialTheme.typography.bodySmall,
            )
        val diagnostics = json.arr("diagnostics").orEmpty().mapNotNull { it as? JsonObject }
        if (diagnostics.isEmpty())
            Text("No diagnostics.", style = MaterialTheme.typography.bodySmall)
        for (diagnostic in diagnostics) {
            val here = diagnostic.str("source") == doc.id
            Column(
                Modifier.fillMaxWidth()
                    .padding(vertical = 4.dp)
                    .then(
                        if (here)
                            Modifier.clickable {
                                onReveal(diagnostic.obj("span")?.long("start")?.toInt() ?: 0)
                            }
                        else Modifier
                    )
                    .testTag("diagnostic")
            ) {
                Text(
                    diagnostic.str("id") ?: "",
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.SemiBold,
                )
                val position = diagnostic.obj("position")
                Text(
                    "${diagnostic.str("source")}:${position?.long("line")}:${position?.long("column")} · " +
                        "${diagnostic.str("stage")} · ${diagnostic.str("default_status")}",
                    style = MaterialTheme.typography.bodySmall,
                    fontFamily = FontFamily.Monospace,
                )
                diagnostic.str("meaning")?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall)
                }
                diagnostic.str("detail")?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
    }
}

@Composable
internal fun StructurePanel(report: Pair<String, JsonObject>?) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(8.dp)) {
        val structure = report?.second?.obj("structure")
        if (structure == null)
            return Text(
                "Inspect shows the imports, declarations and execution plan the PC's engine " +
                    "derived."
            )
        val imports = structure.arr("imports").orEmpty().mapNotNull { it as? JsonObject }
        if (imports.isNotEmpty()) {
            Text("Imports", fontWeight = FontWeight.SemiBold)
            imports.forEach {
                Text(
                    "${it.str("kind")} ${it.str("id")} — ${it.str("outcome")}",
                    fontFamily = FontFamily.Monospace,
                )
            }
        }
        val declarations =
            report.second.obj("navigation")?.arr("declarations").orEmpty().mapNotNull {
                it as? JsonObject
            }
        if (declarations.isNotEmpty()) {
            Text("Declarations (${declarations.size})", fontWeight = FontWeight.SemiBold)
            declarations.forEach {
                Text("${it.str("block")}  ${it.str("id")}", fontFamily = FontFamily.Monospace)
            }
        }
        val plan = structure.arr("plan").orEmpty().mapNotNull { it as? JsonObject }
        Text(
            "Execution plan (${plan.size} nodes)",
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.testTag("plan"),
        )
        plan.forEach { node ->
            Text(
                "${node.str("block")}  ${node.str("id") ?: ""}  ${node.str("operation") ?: ""}",
                fontFamily = FontFamily.Monospace,
            )
        }
    }
}

@Composable
internal fun RunPanel(run: RunState?, controller: WorkspaceController) {
    val colors = LocalLclColors.current
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(8.dp)) {
        if (run == null)
            return Text(
                "Run asks the PC to run this document. It pauses before every effect for you to " +
                    "allow or deny it here."
            )
        val state =
            when {
                run.connectionLost ->
                    "connection lost — the run continues on the PC and is followed again on " +
                        "reconnect"
                run.paused != null -> "paused · waiting for you"
                run.finished -> "finished"
                else -> "running"
            }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "${run.run}: $state",
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f).testTag("run_state"),
            )
            if (!run.finished)
                TextButton(onClick = { controller.answer("cancel") }) { Text("Stop") }
            else TextButton(onClick = controller::dismissRun) { Text("Clear") }
        }
        run.failed?.let { Text(it, color = colors.bad) }
        for (event in run.events) {
            val verdict = event.data.str("permission") ?: event.data.str("outcome") ?: ""
            Text(
                "${event.name}  ${event.data.str("operation") ?: ""}  $verdict",
                fontFamily = FontFamily.Monospace,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        val completion = run.report?.obj("completion")
        if (completion != null) {
            val status = completion.str("terminal_status") ?: ""
            Text(
                status,
                color = if (status == "status.succeeded") colors.good else colors.bad,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.testTag("terminal_status"),
            )
            completion.str("reason")?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            completion
                .arr("outputs")
                .orEmpty()
                .mapNotNull { it as? JsonObject }
                .forEach {
                    Text(
                        "${it.str("id")} = ${(it["value"] as? JsonPrimitive)?.content ?: "—"}  " +
                            "(${it.str("publication")})",
                        fontFamily = FontFamily.Monospace,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
        } else if (run.finished && run.report != null) {
            Text("The run did not reach completion; Diagnostics says where it stopped.")
        }
    }
}
