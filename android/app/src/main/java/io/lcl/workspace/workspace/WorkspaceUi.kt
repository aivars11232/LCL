package io.lcl.workspace.workspace

import io.lcl.workspace.local.LocalProjects
import kotlinx.serialization.json.JsonObject

data class ProjectInfo(
    val id: String,
    val name: String,
    val root: String,
    val isDefault: Boolean,
    val local: Boolean = false,
)

/** One listed file or folder, and the SPECIFICATION KIND the PC's engine read in the file. */
data class TreeEntry(val id: String, val directory: Boolean, val kind: String? = null) {
    /** The last part of the path: what the explorer shows. */
    val name: String
        get() = id.substringAfterLast('/')
}

/**
 * The exact starting text the PC would write for a new file of [role] named [path], and its
 * SHA-256. Creation sends [digest] back, and the PC creates the file only if it would still write
 * exactly this text.
 */
data class ScaffoldPreview(
    val path: String,
    val role: String,
    val mode: String,
    val text: String,
    val digest: String,
)

/** A file role the PC's Core 0.3.0 engine defines. */
data class RoleInfo(val role: String, val label: String)

/** One file of a project, as the PC's engine judged it. */
data class ReadinessFile(
    val path: String,
    val unit: String?,
    val role: String,
    val required: Boolean,
    val status: String,
)

/**
 * A project's readiness: the PC engine's `validate` of the entry on disk. [status] is `ready` only
 * when the engine admitted the project.
 */
data class Readiness(
    val entry: String,
    val status: String,
    val files: List<ReadinessFile>,
    val diagnostics: List<String>,
)

data class RunEvent(val name: String, val data: JsonObject)

/** One run on the PC, as this device follows it. */
data class RunState(
    val project: String,
    val run: String,
    val document: String,
    /** The document revision the run was given; its report describes that text. */
    val revision: Long = 0,
    val events: List<RunEvent> = emptyList(),
    /** The pause the run is waiting at, for this device to answer. */
    val paused: JsonObject? = null,
    val report: JsonObject? = null,
    val failed: String? = null,
    val finished: Boolean = false,
    /** Run events received, so a reconnect resumes from the first one missed. */
    val seen: Long = 0,
    val connectionLost: Boolean = false,
)

/**
 * Host permissions for one run: what this PC may do for it, beyond what the document authorizes.
 */
data class RunGrants(
    val read: List<String> = emptyList(),
    val write: List<String> = emptyList(),
    val program: List<String> = emptyList(),
    val host: List<String> = emptyList(),
    val inputs: List<String> = emptyList(),
)

data class WorkspaceUi(
    val projects: List<ProjectInfo> = emptyList(),
    val project: ProjectInfo? = null,
    /**
     * Each project's explorer, by project id, for as long as the app runs: what the PC has listed,
     * folder by folder.
     */
    val explorers: Map<String, Explorer> = emptyMap(),
    val documents: List<OpenDocument> = emptyList(),
    val active: String? = null,
    /** The last Check, Validate or Inspect report, per document key, and which it was. */
    val reports: Map<String, Pair<String, JsonObject>> = emptyMap(),
    val run: RunState? = null,
    val about: JsonObject? = null,
    /** The PC's default ending for new documents. */
    val defaultEnding: String = LclNames.SUFFIX,
    val busy: Boolean = false,
    /** The file roles the PC offers for New; empty when the PC has no Core 0.3.0. */
    val roles: List<RoleInfo> = emptyList(),
    /** The last project readiness asked for, shown until dismissed. */
    val readiness: Readiness? = null,
    /** Bumped after every sync, so what shows a local project's state reads the records again. */
    val syncEpoch: Int = 0,
) {
    val activeDocument: OpenDocument?
        get() = documents.firstOrNull { key(it) == active }

    /** The current project's explorer: empty until the PC listed its root. */
    val explorer: Explorer
        get() = project?.let { explorers[it.id] } ?: Explorer()

    /** This state without anything of a PC: the phone's own projects and documents stay. */
    fun withoutPc(): WorkspaceUi {
        val documents = documents.filter { LocalProjects.isLocal(it.project) }
        return WorkspaceUi(
            projects = projects.filter { it.local },
            project = project?.takeIf { it.local },
            explorers = explorers.filterKeys(LocalProjects::isLocal),
            documents = documents,
            active = active?.takeIf { key -> documents.any { key(it) == key } },
            reports = reports.filterKeys { key -> documents.any { key(it) == key } },
            defaultEnding = defaultEnding,
        )
    }

    companion object {
        fun key(doc: OpenDocument) = "${doc.project}/${doc.id}"
    }
}
