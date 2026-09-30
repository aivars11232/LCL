package io.lcl.workspace.workspace

import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.editor.ByteSpan
import io.lcl.workspace.local.LocalConflict
import io.lcl.workspace.local.LocalProjects
import io.lcl.workspace.local.LocalRefused
import io.lcl.workspace.remote.RemoteException
import io.lcl.workspace.remote.Reply
import io.lcl.workspace.remote.arr
import io.lcl.workspace.remote.bool
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.str
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** A project this device can work in: one the connected PC shares, or one on this phone ([local]). */
data class ProjectInfo(val id: String, val name: String, val root: String, val isDefault: Boolean, val local: Boolean = false)

/** One listed file or folder, and the SPECIFICATION KIND the PC's engine read in the file. */
data class TreeEntry(val id: String, val directory: Boolean, val kind: String? = null) {
    /** The last part of the path: what the explorer shows. */
    val name: String get() = id.substringAfterLast('/')
}

/**
 * The exact starting text the PC would write for a new file of [role] named
 * [path], and its SHA-256. Creation sends [digest] back, and the PC creates
 * the file only if it would still write exactly this text.
 */
data class ScaffoldPreview(val path: String, val role: String, val mode: String, val text: String, val digest: String)

/** A file role the PC's Core 0.3.0 engine defines. */
data class RoleInfo(val role: String, val label: String)

/** One file of a project, as the PC's engine judged it. */
data class ReadinessFile(val path: String, val unit: String?, val role: String, val required: Boolean, val status: String)

/**
 * A project's readiness: the PC engine's `validate` of the entry on disk.
 * [status] is `ready` only when the engine admitted the project.
 */
data class Readiness(val entry: String, val status: String, val files: List<ReadinessFile>, val diagnostics: List<String>)

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

/** Host permissions for one run: what this PC may do for it, beyond what the document authorizes. */
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
    /** Each project's explorer, by project id, for as long as the app runs: what the PC has listed, folder by folder. */
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
) {
    val activeDocument: OpenDocument? get() = documents.firstOrNull { key(it) == active }

    /** The current project's explorer: empty until the PC listed its root. */
    val explorer: Explorer get() = project?.let { explorers[it.id] } ?: Explorer()

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

/**
 * The workspace as this device sees it, kept in step with the PC.
 *
 * The PC decides everything about LCL: what a document means, whether it is
 * valid, what a run may do. This class only asks, shows the answers, and keeps
 * the device's copies honest about which revision they started from.
 */
class WorkspaceController(
    private val connection: ConnectionManager,
    private val scope: CoroutineScope,
    /** How long typing must pause before the PC is asked to analyse. */
    private val analysisDelayMs: Long = 400,
    /** The projects on this phone; none when the app has no private storage (tests without one). */
    private val local: LocalProjects? = null,
    /** Where the phone's own files are read and written. */
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
) {
    private val _ui = MutableStateFlow(WorkspaceUi())
    val ui: StateFlow<WorkspaceUi> = _ui
    private val _messages = MutableSharedFlow<String>(extraBufferCapacity = 16)
    val messages: SharedFlow<String> = _messages
    private var analysis: Job? = null
    /** The PC the workspace shows. */
    private var pcId: String? = null
    /** The projects the connected PC shares, as last listed. */
    private var pcProjects: List<ProjectInfo> = emptyList()

    init {
        scope.launch { refreshLocalProjects() }
        scope.launch {
            connection.state.collect { state ->
                val now = state.pcOrNull?.pcId
                if (now != pcId) {
                    // Another PC, or none: nothing of the last one stays on screen
                    // or is ever sent to the next. The phone's own projects and
                    // documents are not the PC's, and stay.
                    pcId = now
                    analysis?.cancel()
                    pcProjects = emptyList()
                    _ui.update { ui -> ui.withoutPc() }
                }
                if (state is ConnectionState.Connected) onConnected()
                else _ui.update { ui -> ui.run?.let { ui.copy(run = it.copy(connectionLost = !it.finished)) } ?: ui }
            }
        }
        scope.launch { connection.events.collect(::onEvent) }
    }

    /** Whether a project id names one on this phone. */
    private fun isLocal(project: String) = LocalProjects.isLocal(project)

    /** The phone's storage, or a refusal said to the person. */
    private fun storage(): LocalProjects? = local ?: run { say("This build keeps no projects on the phone."); null }

    private fun say(message: String) {
        _messages.tryEmit(message)
    }

    private suspend fun ask(op: String, fields: JsonObject = JsonObject(emptyMap())): Reply? = try {
        connection.request(op, fields)
    } catch (e: CancellationException) {
        throw e // superseded (more typing, a closed screen): not a failure to report
    } catch (e: RemoteException) {
        say(e.message ?: "Not connected to the PC.")
        null
    } catch (e: Exception) {
        say("The PC did not answer: ${e.message}")
        null
    }

    private fun fields(project: String, vararg pairs: Pair<String, Any?>): JsonObject = buildJsonObject {
        put("project", project)
        for ((k, v) in pairs) when (v) {
            is String -> put(k, v)
            is Long -> put(k, v)
            is Int -> put(k, v)
            is Boolean -> put(k, v)
            is JsonObject -> put(k, v)
            is JsonArray -> put(k, v)
        }
    }

    // -------------------------------------------------------------- connection

    /** After every (re)connect: what the PC has now, and what changed while away. */
    private suspend fun onConnected() {
        _ui.update { it.copy(about = null) }
        loadProjects()
        ask("about")?.takeIf { it.ok }?.let { reply -> _ui.update { it.copy(about = reply.obj) } }
        val project = _ui.value.project ?: return
        ask("settings", fields(project.id))?.takeIf { it.ok }?.let { reply ->
            val ending = reply.obj.str("default_extension")
            if (ending == LclNames.SUFFIX || ending == LclNames.TEXT_SUFFIX) _ui.update { it.copy(defaultEnding = ending) }
        }
        refreshTree()
        reconcileDocuments()
        resumeRun()
    }

    /** Documents open here are checked against the PC's current revisions. */
    private suspend fun reconcileDocuments() {
        for (doc in _ui.value.documents) {
            val reply = ask("open", fields(doc.project, "document" to doc.id)) ?: continue
            if (!reply.ok) {
                replaceDocument(doc) { it.copy(deletedOnPc = reply.status == 404) }
                continue
            }
            val digest = reply.obj.str("digest") ?: continue
            val text = reply.obj.str("text")
            replaceDocument(doc) { current ->
                when (current.remoteChange(digest)) {
                    RemoteChange.SAME -> current
                    RemoteChange.REFRESH -> current.reloaded(text ?: current.text, digest)
                    RemoteChange.CONFLICT -> current.inConflict(text, digest)
                    RemoteChange.DELETED -> current.copy(deletedOnPc = true)
                }
            }
        }
    }

    /** A run that was going when the connection dropped is followed again from the first event missed. */
    private suspend fun resumeRun() {
        val run = _ui.value.run ?: return
        if (run.finished) return
        val reply = ask("follow", fields(run.project, "run" to run.run, "from" to run.seen))
        _ui.update { ui ->
            ui.copy(
                run = ui.run?.copy(
                    connectionLost = false,
                    failed = if (reply?.ok == false) "The PC no longer has this run: ${reply.error}" else ui.run.failed,
                    finished = ui.run.finished || reply?.ok == false,
                ),
            )
        }
    }

    // ---------------------------------------------------------------- projects

    suspend fun loadProjects() {
        val reply = ask("projects") ?: return
        pcProjects = reply.obj.arr("projects")?.mapNotNull { element ->
            val p = element as? JsonObject ?: return@mapNotNull null
            ProjectInfo(p.str("id") ?: return@mapNotNull null, p.str("name") ?: "", p.str("root") ?: "", p.bool("default") == true)
        } ?: emptyList()
        mergeProjects(preferPc = true)
    }

    /** The projects on this phone, listed again: after one is made or removed, and at start. */
    suspend fun refreshLocalProjects() {
        val storage = local ?: return
        val found = runCatching { withContext(io) { storage.projects() } }.getOrElse { emptyList() }
        localProjects = found.map { ProjectInfo(it.id, it.name, "On this phone", false, local = true) }
        mergeProjects(preferPc = false)
    }

    private var localProjects: List<ProjectInfo> = emptyList()

    /** The PC's projects and the phone's, as one list; the current one stays chosen when it is still there. */
    private fun mergeProjects(preferPc: Boolean) {
        val projects = pcProjects + localProjects
        _ui.update { ui ->
            val kept = ui.project?.let { current -> projects.firstOrNull { it.id == current.id } }
            val chosen = kept ?: if (preferPc) (pcProjects.firstOrNull() ?: localProjects.firstOrNull()) else (ui.project ?: projects.firstOrNull())
            ui.copy(projects = projects, project = chosen)
        }
    }

    /** Make a project on this phone and open it; no PC is needed. */
    fun createLocalProject(name: String) {
        val storage = storage() ?: return
        scope.launch {
            val made = try {
                withContext(io) { storage.create(name.trim()) }
            } catch (e: LocalRefused) {
                return@launch say(e.message ?: "Not created.")
            }
            refreshLocalProjects()
            selectProject(made.id)
            say("Created ${made.name} on this phone")
        }
    }

    /** Whether every document of the local project [id] is on a PC as it is now, so the phone's copy can go. */
    fun localProjectSynced(id: String): Boolean =
        local?.let { runCatching { it.fullySynced(LocalProjects.nameOf(id)) }.getOrDefault(false) } ?: false

    /**
     * Remove a project from this phone. Only when every document of it is on a
     * PC as it is now, and only because the person asked: nothing removes a
     * local copy on its own, and never before a PC confirmed the bytes.
     */
    fun removeLocalProject(id: String) {
        val storage = storage() ?: return
        val name = LocalProjects.nameOf(id)
        scope.launch {
            if (!withContext(io) { storage.fullySynced(name) }) {
                return@launch say("$name is not fully synced to a PC, so it stays on this phone.")
            }
            try {
                withContext(io) { storage.deleteProject(name) }
            } catch (e: LocalRefused) {
                return@launch say(e.message ?: "Not removed.")
            }
            _ui.update { ui ->
                val rest = ui.documents.filter { it.project != id }
                ui.copy(
                    documents = rest,
                    active = ui.active?.takeIf { key -> rest.any { WorkspaceUi.key(it) == key } },
                    explorers = ui.explorers - id,
                    project = ui.project?.takeIf { it.id != id },
                )
            }
            refreshLocalProjects()
            say("Removed $name from this phone")
        }
    }

    /** Switch to another shared project: its explorer starts over, and nothing of the last one shows. */
    fun selectProject(id: String) {
        val project = _ui.value.projects.firstOrNull { it.id == id } ?: return
        _ui.update { it.copy(project = project, explorers = it.explorers - project.id) }
        scope.launch { refreshTree() }
    }

    private fun updateExplorer(project: String, change: (Explorer) -> Explorer) = _ui.update {
        it.copy(explorers = it.explorers + (project to change(it.explorers[project] ?: Explorer())))
    }

    private fun entriesOf(reply: Reply): List<TreeEntry> = reply.obj.arr("entries")?.mapNotNull { element ->
        val e = element as? JsonObject ?: return@mapNotNull null
        TreeEntry(e.str("id") ?: return@mapNotNull null, e.bool("directory") == true, e.str("kind"))
    } ?: emptyList()

    /**
     * Read one folder of [project] from the PC — its direct children and
     * nothing below them — into the explorer. A PC from before `children`
     * existed (LCL 0.5.0) answers 400 for it; then its whole `tree` is asked
     * for once, and this folder's children are taken out of it.
     */
    private suspend fun loadFolder(project: String, folder: String): Boolean {
        if (isLocal(project)) {
            val storage = storage() ?: return false
            val loaded = try {
                withContext(io) { storage.children(LocalProjects.nameOf(project), folder) }
            } catch (e: LocalRefused) {
                updateExplorer(project) { it.without(folder) }
                say(e.message ?: "$folder could not be listed.")
                return false
            }
            updateExplorer(project) { it.with(folder, loaded) }
            return true
        }
        val reply = ask("children", fields(project, "parent" to folder)) ?: return false
        if (reply.ok) {
            updateExplorer(project) { it.with(folder, LoadedFolder(entriesOf(reply), reply.obj.bool("truncated") == true)) }
            return true
        }
        if (reply.status == 400 && reply.error?.contains("unknown operation") == true) {
            val whole = ask("tree", fields(project)) ?: return false
            if (!whole.ok) { say(whole.error ?: "The project could not be listed."); return false }
            val all = entriesOf(whole)
            val direct = all.filter { FileTree.parentOf(it.id) == folder }
            val children = direct.filter { it.directory } + direct.filter { !it.directory }
            updateExplorer(project) { it.with(folder, LoadedFolder(children, whole.obj.bool("truncated") == true && folder.isEmpty())) }
            return true
        }
        if (reply.status == 404) updateExplorer(project) { it.without(folder) }
        say(reply.error ?: "$folder could not be listed.")
        return false
    }

    /**
     * Read the root and every unfolded folder again, for what changed on the
     * PC. A folded folder is not read: it is read when it is unfolded. Tabs,
     * unsaved text and folds stay as they are; a folder that is gone leaves
     * the tree with everything below it.
     */
    suspend fun refreshTree() {
        val project = _ui.value.project ?: return
        if (!project.local && connection.state.value !is ConnectionState.Connected) return
        if (!loadFolder(project.id, "")) return
        val unfolded = _ui.value.explorers[project.id]?.expanded.orEmpty().sortedBy { it.count { c -> c == '/' } }
        for (folder in unfolded) {
            val explorer = _ui.value.explorers[project.id] ?: Explorer()
            if (!explorer.isFolder(folder)) { updateExplorer(project.id) { it.without(folder) }; continue }
            loadFolder(project.id, folder)
        }
        if (!project.local) loadRoles() else _ui.update { it.copy(roles = emptyList()) }
    }

    /** Make one empty folder in the current project; it shows at once. */
    fun createFolder(folder: String) {
        val project = _ui.value.project ?: return
        scope.launch {
            val made = if (project.local) {
                val storage = storage() ?: return@launch
                try {
                    withContext(io) { storage.mkdir(LocalProjects.nameOf(project.id), folder) }
                } catch (e: LocalRefused) {
                    return@launch say(e.message ?: "Not created.")
                }
            } else {
                val reply = ask("mkdir", fields(project.id, "folder" to folder)) ?: return@launch
                if (!reply.ok) return@launch say(reply.error ?: "Not created.")
                reply.obj.str("id") ?: folder
            }
            // Its parent is read again, so it shows at once, unfolded and empty.
            revealFolders(project.id, made, fresh = true)
            if (loadFolder(project.id, made)) updateExplorer(project.id) { it.copy(expanded = it.expanded + made) }
            say("Created folder $made")
        }
    }

    /** Unfold every folder on the way to [id], reading the ones never read; with [fresh], reading them again. */
    private suspend fun revealFolders(project: String, id: String, fresh: Boolean = false) {
        for (folder in listOf("") + FileTree.ancestors(id)) {
            val known = _ui.value.explorers[project]?.folders?.containsKey(folder) == true
            if ((fresh || !known) && !loadFolder(project, folder)) return
            if (folder.isNotEmpty()) updateExplorer(project) { it.copy(expanded = it.expanded + folder) }
        }
    }

    /** The roles New can create, from the PC's engine. */
    private suspend fun loadRoles() {
        val project = _ui.value.project ?: return
        val reply = ask("roles", fields(project.id)) ?: return
        val roles = if (reply.ok && reply.obj.bool("available") == true) {
            reply.obj.arr("roles")?.mapNotNull { element ->
                val r = element as? JsonObject ?: return@mapNotNull null
                RoleInfo(r.str("role") ?: return@mapNotNull null, r.str("label") ?: "")
            } ?: emptyList()
        } else emptyList()
        _ui.update { it.copy(roles = roles) }
    }

    /** Ask the PC's engine whether the project [entry] names is ready to run. */
    fun readiness(entry: String) {
        val project = _ui.value.project ?: return
        scope.launch {
            val reply = ask("project", fields(project.id, "entry" to entry)) ?: return@launch
            if (!reply.ok) return@launch say(reply.error ?: "Readiness could not be read.")
            _ui.update { it.copy(readiness = readinessOf(entry, reply.obj)) }
        }
    }

    fun dismissReadiness() = _ui.update { it.copy(readiness = null) }

    // --------------------------------------------------------------- documents

    /** Fold [folder] in the current project's tree if it is unfolded, or unfold it, reading its children from the PC. */
    fun toggleFolder(folder: String) {
        val project = _ui.value.project ?: return
        if (folder in _ui.value.explorer.expanded) {
            updateExplorer(project.id) { it.copy(expanded = it.expanded - folder) }
            return
        }
        scope.launch {
            if (loadFolder(project.id, folder)) updateExplorer(project.id) { it.copy(expanded = it.expanded + folder) }
        }
    }

    fun open(id: String) {
        val project = _ui.value.project ?: return
        val key = "${project.id}/$id"
        scope.launch { revealFolders(project.id, id) }
        if (_ui.value.documents.any { WorkspaceUi.key(it) == key }) {
            _ui.update { it.copy(active = key) }
            return
        }
        scope.launch {
            if (project.local) {
                val storage = storage() ?: return@launch
                val read = try {
                    withContext(io) { storage.read(LocalProjects.nameOf(project.id), id) }
                } catch (e: LocalRefused) {
                    return@launch say(e.message ?: "$id could not be opened.")
                }
                val doc = OpenDocument(project.id, id, read.text, read.text, read.digest)
                _ui.update { it.copy(documents = it.documents + doc, active = key) }
                return@launch
            }
            val reply = ask("open", fields(project.id, "document" to id)) ?: return@launch
            if (!reply.ok) return@launch say(reply.error ?: "$id could not be opened.")
            val text = reply.obj.str("text") ?: ""
            val doc = OpenDocument(project.id, id, text, text, reply.obj.str("digest") ?: "")
            _ui.update { it.copy(documents = it.documents + doc, active = key) }
            analyse(doc)
        }
    }

    fun activate(key: String) {
        _ui.value.documents.firstOrNull { WorkspaceUi.key(it) == key }?.let { doc -> scope.launch { revealFolders(doc.project, doc.id) } }
        _ui.update { it.copy(active = key) }
    }

    fun close(key: String) {
        val doc = _ui.value.documents.firstOrNull { WorkspaceUi.key(it) == key } ?: return
        _ui.update { ui ->
            val rest = ui.documents - doc
            ui.copy(
                documents = rest,
                active = if (ui.active == key) rest.lastOrNull()?.let(WorkspaceUi::key) else ui.active,
                reports = ui.reports - key,
            )
        }
        scope.launch { ask("close", fields(doc.project, "document" to doc.id)) }
    }

    private fun replaceDocument(doc: OpenDocument, change: (OpenDocument) -> OpenDocument) {
        val key = WorkspaceUi.key(doc)
        _ui.update { ui -> ui.copy(documents = ui.documents.map { if (WorkspaceUi.key(it) == key) change(it) else it }) }
    }

    private fun current(key: String): OpenDocument? = _ui.value.documents.firstOrNull { WorkspaceUi.key(it) == key }

    /** Typing: the text changes on screen at once; the PC is asked once typing pauses. */
    fun edit(key: String, text: String) {
        val doc = current(key) ?: return
        val edited = doc.edited(text)
        if (edited === doc) return
        replaceDocument(doc) { edited }
        // A document on this phone has no engine to ask.
        if (isLocal(doc.project)) return
        analysis?.cancel()
        analysis = scope.launch {
            delay(analysisDelayMs)
            current(key)?.let { analyse(it) }
        }
    }

    /** Ask the PC for tokens and diagnostics for this exact revision. */
    private suspend fun analyse(doc: OpenDocument) {
        val revision = doc.revision
        val key = WorkspaceUi.key(doc)
        ask("tokens", fields(doc.project, "document" to doc.id, "text" to doc.text))?.takeIf { it.ok }?.let { reply ->
            val spans = reply.obj.arr("tokens")?.mapNotNull { t ->
                val o = t as? JsonObject ?: return@mapNotNull null
                ByteSpan((o.long("start") ?: return@mapNotNull null).toInt(), (o.long("end") ?: return@mapNotNull null).toInt(), o.str("class") ?: "")
            } ?: emptyList()
            current(key)?.let { now -> replaceDocument(now) { it.withTokens(spans, revision) } }
        }
        ask("inspect", fields(doc.project, "document" to doc.id, "text" to doc.text))?.takeIf { it.ok }?.let { reply ->
            applyReport(key, "Inspect", reply.obj, revision)
        }
    }

    /** A report is kept only for the revision it describes. */
    private fun applyReport(key: String, kind: String, report: JsonObject, revision: Long) {
        val doc = current(key) ?: return
        if (doc.revision != revision) return // describes text that is no longer there
        replaceDocument(doc) { it.withMarks(diagnosticSpans(report, doc.id), revision) }
        _ui.update { it.copy(reports = it.reports + (key to (kind to report))) }
    }

    /** Check, Validate or Inspect the text on screen, through the PC's engine. */
    fun analyse(op: String) {
        val doc = _ui.value.activeDocument ?: return
        if (isLocal(doc.project)) return say("${op.replaceFirstChar { it.uppercase() }} needs the PC's engine: sync this project to a PC, or open the PC's copy.")
        scope.launch {
            val revision = doc.revision
            val reply = ask(op, fields(doc.project, "document" to doc.id, "text" to doc.text)) ?: return@launch
            if (!reply.ok) return@launch say(reply.error ?: "$op failed.")
            val kind = op.replaceFirstChar { it.uppercase() }
            applyReport(WorkspaceUi.key(doc), kind, reply.obj, revision)
            say("$kind: ${reply.obj.str("outcome") ?: "done"}")
        }
    }

    /** Save over exactly the revision this copy started from, or stop. */
    fun save(key: String = _ui.value.active ?: "") {
        val doc = current(key) ?: return
        scope.launch {
            val submitted = doc.text
            val revision = doc.revision
            if (isLocal(doc.project)) {
                val storage = storage() ?: return@launch
                try {
                    val written = withContext(io) { storage.write(LocalProjects.nameOf(doc.project), doc.id, submitted, base = doc.base) }
                    current(key)?.let { now -> replaceDocument(now) { it.savedAs(submitted, written.digest, false, revision) } }
                    say("Saved ${doc.name} on this phone")
                } catch (e: LocalConflict) {
                    current(key)?.let { now -> replaceDocument(now) { it.inConflict(e.text, e.digest) } }
                    say("Not saved: ${doc.name} changed on this phone since it was opened.")
                } catch (e: LocalRefused) {
                    say("Not saved: ${e.message}")
                }
                return@launch
            }
            val reply = ask("save", fields(doc.project, "document" to doc.id, "base" to doc.base, "text" to submitted)) ?: return@launch
            when {
                reply.ok -> {
                    val digest = reply.obj.str("digest") ?: return@launch
                    val added = reply.obj.bool("final_line_feed_added") == true
                    current(key)?.let { now -> replaceDocument(now) { it.savedAs(submitted, digest, added, revision) } }
                    say("Saved ${doc.name}" + if (added) " (a final line feed was added)" else "")
                }
                reply.status == 409 -> {
                    val digest = reply.obj.str("digest")
                    current(key)?.let { now ->
                        replaceDocument(now) { it.inConflict(reply.obj.str("text"), digest ?: it.base) }
                    }
                    say("Not saved: ${doc.name} changed on the PC. Reload it or keep yours.")
                }
                reply.status == 404 -> {
                    current(key)?.let { now -> replaceDocument(now) { it.copy(deletedOnPc = true) } }
                    say("Not saved: ${doc.name} no longer exists on the PC.")
                }
                else -> say("Not saved: ${reply.error}")
            }
        }
    }

    /** Replace this copy with the PC's current file. */
    fun reload(key: String = _ui.value.active ?: "") {
        val doc = current(key) ?: return
        scope.launch {
            if (isLocal(doc.project)) {
                val storage = storage() ?: return@launch
                val read = try {
                    withContext(io) { storage.read(LocalProjects.nameOf(doc.project), doc.id) }
                } catch (e: LocalRefused) {
                    return@launch say(e.message ?: "${doc.name} could not be reloaded.")
                }
                current(key)?.let { now -> replaceDocument(now) { it.reloaded(read.text, read.digest) } }
                return@launch
            }
            val reply = ask("open", fields(doc.project, "document" to doc.id)) ?: return@launch
            if (!reply.ok) return@launch say(reply.error ?: "${doc.name} could not be reloaded.")
            current(key)?.let { now ->
                replaceDocument(now) { it.reloaded(reply.obj.str("text") ?: "", reply.obj.str("digest") ?: "") }
            }
            current(key)?.let { analyse(it) }
        }
    }

    /** Resolve a conflict by keeping this device's text; the next save replaces the PC's newer revision on purpose. */
    fun keepMine(key: String) {
        current(key)?.let { doc -> replaceDocument(doc) { it.keepMine() } }
    }

    /** Delete a document of a local project, only while it still holds the bytes shown; its tab closes with it. */
    fun deleteLocal(key: String) {
        val doc = current(key) ?: return
        if (!isLocal(doc.project)) return
        val storage = storage() ?: return
        scope.launch {
            try {
                withContext(io) { storage.delete(LocalProjects.nameOf(doc.project), doc.id, doc.base) }
            } catch (e: LocalRefused) {
                return@launch say(e.message ?: "Not deleted.")
            }
            close(key)
            revealFolders(doc.project, doc.id, fresh = true)
            say("Deleted ${doc.name} from this phone")
        }
    }

    // --------------------------------------------------------------------- sync

    /**
     * Find out, without writing anything, what syncing [localProject] (or only
     * [ids] of it) into [folder] of [pcProject] would meet: for each document,
     * whether the PC holds nothing at its destination, the same bytes, or
     * different ones. Needs the PC; fails with the reason otherwise.
     */
    suspend fun planSync(localProject: ProjectInfo, pcProject: ProjectInfo, folder: String, ids: List<String>? = null): Result<SyncPlan> {
        val storage = local ?: return Result.failure(LocalRefused("This build keeps no projects on the phone."))
        if (!localProject.local || pcProject.local) return Result.failure(LocalRefused("Sync goes from a project on this phone to one on the PC."))
        if (connection.state.value !is ConnectionState.Connected) return Result.failure(LocalRefused("Connect to the PC first."))
        val where = folder.trim().trim('/')
        if (where.split('/').any { it == ".." || it.startsWith(".") }) return Result.failure(LocalRefused("\"$where\" is not a folder inside the PC project."))
        val name = LocalProjects.nameOf(localProject.id)
        val documents = try {
            withContext(io) { ids ?: storage.documents(name) }
        } catch (e: LocalRefused) {
            return Result.failure(e)
        }
        val items = mutableListOf<SyncItem>()
        for (id in documents) {
            val read = try {
                withContext(io) { storage.read(name, id) }
            } catch (e: LocalRefused) {
                return Result.failure(e)
            }
            val destination = if (where.isEmpty()) id else "$where/$id"
            val reply = try {
                connection.request("open", fields(pcProject.id, "document" to destination))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                return Result.failure(LocalRefused("The PC did not answer: ${e.message}"))
            }
            items += when {
                reply.ok -> {
                    val pcDigest = reply.obj.str("digest") ?: ""
                    val state = if (pcDigest == read.digest) SyncState.IDENTICAL else SyncState.DIFFERENT
                    SyncItem(id, destination, read.text, read.digest, state, pcDigest, reply.obj.str("text"))
                }
                reply.status == 404 -> SyncItem(id, destination, read.text, read.digest, SyncState.ABSENT)
                else -> return Result.failure(LocalRefused("The PC refused $destination: ${reply.error ?: reply.status}"))
            }
        }
        return Result.success(SyncPlan(localProject, pcProject, where, items))
    }

    /**
     * Do a planned sync. Every document is written the way the person chose
     * for it, and counts as synced only once the PC's answer names the bytes
     * sent (their digest, with the final line feed the PC may add). A
     * connection lost midway stops the rest; nothing local changes on any
     * failure. With [removeAfter], the local copy goes only when every
     * document of the project is confirmed on the PC.
     */
    suspend fun runSync(plan: SyncPlan, choices: Map<String, SyncChoice>, removeAfter: Boolean): SyncResult {
        val storage = local ?: return SyncResult(emptyList(), complete = false, removed = false)
        val name = LocalProjects.nameOf(plan.project.id)
        val outcomes = mutableListOf<SyncOutcome>()
        // Every folder on the way to a destination, shallowest first; one that
        // exists already is fine.
        val folders = plan.items.flatMap { FileTree.ancestors(it.destination) }.distinct().sortedBy { it.count { c -> c == '/' } }
        val unreachable = mutableSetOf<String>()
        var lost: String? = null
        for (folder in folders) {
            if (lost != null) break
            if (unreachable.any { folder == it || folder.startsWith("$it/") }) continue
            val reply = try {
                connection.request("mkdir", fields(plan.pcProject.id, "folder" to folder))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                lost = e.message ?: "connection lost"; break
            }
            if (!reply.ok && reply.status != 409) unreachable += folder
        }
        for (item in plan.items) {
            val parent = FileTree.parentOf(item.destination)
            if (lost != null) {
                outcomes += SyncOutcome(item.id, item.destination, false, "not attempted: $lost"); continue
            }
            if (unreachable.any { parent == it || parent.startsWith("$it/") }) {
                outcomes += SyncOutcome(item.id, item.destination, false, "its folder could not be made on the PC"); continue
            }
            val choice = choices[item.id]
            val (op, request, destination) = when (item.state) {
                SyncState.IDENTICAL -> {
                    withContext(io) { storage.recordSynced(name, item.id, item.localDigest, plan.pcProject.id, item.destination, item.pcDigest ?: item.localDigest, now()) }
                    outcomes += SyncOutcome(item.id, item.destination, true, "already on the PC"); continue
                }
                SyncState.ABSENT -> Triple("create", fields(plan.pcProject.id, "name" to item.destination, "text" to item.text), item.destination)
                SyncState.DIFFERENT -> when (choice) {
                    SyncChoice.REPLACE -> Triple("save", fields(plan.pcProject.id, "document" to item.destination, "base" to (item.pcDigest ?: ""), "text" to item.text), item.destination)
                    SyncChoice.RENAME -> Triple("create", fields(plan.pcProject.id, "name" to item.renamed, "text" to item.text), item.renamed)
                    SyncChoice.KEEP_PC, null -> {
                        outcomes += SyncOutcome(item.id, item.destination, false, "kept the PC's version; the phone's copy is not on the PC"); continue
                    }
                }
            }
            val reply = try {
                connection.request(op, request)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                lost = e.message ?: "connection lost"
                outcomes += SyncOutcome(item.id, destination, false, "not confirmed: ${lost}"); continue
            }
            if (!reply.ok) {
                val why = when (reply.status) {
                    409 -> "changed on the PC since it was checked, or exists; nothing was written"
                    else -> reply.error ?: "refused (${reply.status})"
                }
                outcomes += SyncOutcome(item.id, destination, false, why); continue
            }
            // Confirmation is the PC's own digest of what it holds now: the
            // bytes sent, or those bytes with the final line feed it adds.
            val confirmed = reply.obj.str("digest")
            val expected = listOf(item.localDigest) + (if (item.text.endsWith("\n")) emptyList() else listOf(LocalProjects.digest(item.text + "\n")))
            if (confirmed == null || confirmed !in expected) {
                outcomes += SyncOutcome(item.id, destination, false, "the PC reported other bytes than were sent; not marked synced"); continue
            }
            withContext(io) { storage.recordSynced(name, item.id, item.localDigest, plan.pcProject.id, destination, confirmed, now()) }
            outcomes += SyncOutcome(item.id, destination, true, if (op == "save") "replaced on the PC" else "created on the PC")
        }
        val complete = outcomes.isNotEmpty() && outcomes.all { it.ok }
        var removed = false
        if (removeAfter && complete && withContext(io) { storage.fullySynced(name) }) {
            removeLocalProject(plan.project.id)
            removed = true
        }
        return SyncResult(outcomes, complete, removed)
    }

    /**
     * The PC's starting text for a new file of [role] called [name] (named as
     * the PC will name it), or null when the PC refuses; the reason is said.
     */
    suspend fun previewScaffold(name: String, role: String, mode: String): ScaffoldPreview? {
        val project = _ui.value.project ?: return null
        val path = LclNames.defaultName(name, _ui.value.defaultEnding)
        val reply = ask("scaffold", fields(project.id, "role" to role, "path" to path, "mode" to mode)) ?: return null
        if (!reply.ok) {
            say(reply.error ?: "No preview.")
            return null
        }
        val text = reply.obj.str("text") ?: return null
        val digest = reply.obj.str("digest") ?: return null
        return ScaffoldPreview(path, role, mode, text, digest)
    }

    /**
     * Create [name]: blank, or — with [role] — a file of that role, whose text
     * the PC writes from its own scaffold (or the role's default Master) in
     * [mode]. The app never holds a copy of a scaffold: it sends only the
     * digest of the preview the person saw ([scaffoldDigest]), and the PC
     * creates nothing when its text has changed since.
     */
    fun create(name: String, role: String? = null, mode: String = "guided", scaffoldDigest: String? = null) {
        val project = _ui.value.project ?: return
        scope.launch {
            val request = if (role != null) {
                fields(project.id, "name" to name, "role" to role, "mode" to mode, "scaffold_digest" to scaffoldDigest)
            } else {
                val seed = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.new\n    NAME: \"New document\"\n" +
                    "    VERSION: \"1.0.0\"\n    KIND: kind.task\n    DOMAIN: \"general\"\n"
                fields(project.id, "name" to name, "text" to seed)
            }
            val id = if (project.local) {
                val storage = storage() ?: return@launch
                val named = LclNames.defaultName(name, _ui.value.defaultEnding)
                val seed = request.str("text") ?: ""
                try {
                    withContext(io) { storage.createFile(LocalProjects.nameOf(project.id), named, seed) }.id
                } catch (e: LocalRefused) {
                    return@launch say(e.message ?: "Not created.")
                }
            } else {
                val reply = ask("create", request) ?: return@launch
                if (!reply.ok) return@launch say(reply.error ?: "Not created.")
                reply.obj.str("id") ?: return@launch
            }
            // Its folder is read again, so it is in the tree at once; nothing
            // else of the project is.
            revealFolders(project.id, id, fresh = true)
            open(id)
        }
    }

    // --------------------------------------------------------------------- run

    fun run(grants: RunGrants) {
        val doc = _ui.value.activeDocument ?: return
        if (isLocal(doc.project)) return say("Run needs the PC's engine: sync this project to a PC first.")
        if (_ui.value.run?.let { !it.finished } == true) return say("A run is already going. Stop it first.")
        scope.launch {
            val list = { items: List<String> -> JsonArray(items.map(::JsonPrimitive)) }
            val grantsJson = buildJsonObject {
                put("read", list(grants.read))
                put("write", list(grants.write))
                put("program", list(grants.program))
                put("host", list(grants.host))
            }
            val reply = ask(
                "run",
                fields(doc.project, "document" to doc.id, "text" to doc.text, "grants" to grantsJson, "inputs" to list(grants.inputs), "break_effects" to true),
            ) ?: return@launch
            if (!reply.ok) return@launch say("Run refused: ${reply.error}")
            val run = reply.obj.str("run") ?: return@launch
            _ui.update { it.copy(run = RunState(doc.project, run, doc.id, revision = doc.revision)) }
        }
    }

    /** Answer the pause: continue (allow), deny this effect, or cancel the run. */
    fun answer(answer: String) {
        val run = _ui.value.run ?: return
        val paused = run.paused ?: if (answer == "cancel") JsonObject(emptyMap()) else return
        scope.launch {
            val reply = ask("answer", fields(run.project, "run" to run.run, "sequence" to (paused.long("sequence") ?: 0L), "answer" to answer))
                ?: return@launch
            if (!reply.ok) return@launch say("That pause is no longer current: ${reply.error}")
            // Clear only the pause that was answered; the run may already be at its next one.
            _ui.update { ui ->
                val current = ui.run ?: return@update ui
                if (current.paused?.long("sequence") == paused.long("sequence")) ui.copy(run = current.copy(paused = null)) else ui
            }
        }
    }

    fun dismissRun() = _ui.update { ui -> if (ui.run?.finished != false) ui.copy(run = null) else ui }

    // ------------------------------------------------------------------ events

    private fun onEvent(event: JsonObject) {
        when (event.str("event")) {
            "run" -> onRunEvent(event)
            "document_changed" -> {
                val project = event.str("project") ?: return
                val id = event.str("document") ?: return
                val doc = current("$project/$id") ?: return
                val digest = event.str("digest")
                when (doc.remoteChange(digest)) {
                    RemoteChange.SAME -> Unit
                    RemoteChange.DELETED -> replaceDocument(doc) { it.copy(deletedOnPc = true) }
                    RemoteChange.REFRESH -> reload(WorkspaceUi.key(doc)).also { say("${doc.name} was changed on the PC and reloaded.") }
                    RemoteChange.CONFLICT -> scope.launch {
                        val reply = ask("open", fields(project, "document" to id))
                        replaceDocument(doc) { it.inConflict(reply?.obj?.str("text"), digest ?: it.base) }
                        say("${doc.name} was changed on the PC while you were editing it.")
                    }
                }
            }
        }
    }

    private fun onRunEvent(event: JsonObject) {
        val run = _ui.value.run ?: return
        if (event.str("run") != run.run || event.str("project") != run.project) return
        val name = event.str("name") ?: return
        val data = event.obj("data") ?: JsonObject(emptyMap())
        _ui.update { ui ->
            val current = ui.run ?: return@update ui
            val seen = if (name == "end") current.seen else current.seen + 1
            val next = when (name) {
                "paused" -> current.copy(paused = data)
                "resumed" -> current.copy(paused = null)
                "report" -> current.copy(report = data)
                "failed" -> current.copy(failed = data.str("error") ?: "The run failed.")
                "end" -> current.copy(finished = true, paused = null)
                else -> current.copy(events = current.events + RunEvent(name, data))
            }.copy(seen = seen)
            ui.copy(run = next)
        }
        if (name == "report") {
            val key = "${run.project}/${run.document}"
            applyReport(key, "Run", data, run.revision)
        }
    }

    companion object {
        /** How a diagnostic is drawn, from the status the engine gave it. */
        fun severity(status: String?): String = when (status) {
            "status.failed", "status.invalid" -> "bad"
            "status.blocked", "status.cancelled" -> "warn"
            else -> "info"
        }

        /** The report's diagnostics for one document, as byte spans with their engine-derived lines. */
        fun diagnosticSpans(report: JsonObject, document: String): List<ByteSpan> =
            report.arr("diagnostics")?.mapNotNull { element ->
                val d = element as? JsonObject ?: return@mapNotNull null
                if (d.str("source") != document) return@mapNotNull null
                val span = d.obj("span") ?: return@mapNotNull null
                ByteSpan(
                    (span.long("start") ?: return@mapNotNull null).toInt(),
                    (span.long("end") ?: return@mapNotNull null).toInt(),
                    severity(d.str("default_status")),
                    d.obj("position")?.long("line")?.toInt(),
                )
            } ?: emptyList()

        /** A readiness from the PC's `validate` report for [entry]. */
        fun readinessOf(entry: String, report: JsonObject): Readiness {
            val project = report.obj("project")
            val diagnostics = report.arr("diagnostics")?.mapNotNull { element ->
                val d = element as? JsonObject ?: return@mapNotNull null
                val line = d.obj("position")?.long("line")
                "${d.str("id") ?: "?"} — ${d.str("source") ?: "?"}${line?.let { ":$it" } ?: ""}"
            } ?: emptyList()
            if (project == null) return Readiness(entry, "invalid", emptyList(), diagnostics)
            val files = listOf(
                ReadinessFile(project.str("entry") ?: entry, project.str("entry"), "kind.project", true, project.str("entry_status") ?: "invalid"),
            ) + (project.arr("parts")?.mapNotNull { element ->
                val part = element as? JsonObject ?: return@mapNotNull null
                ReadinessFile(
                    part.str("unit") ?: part.str("source") ?: "?",
                    part.str("unit"),
                    part.str("kind") ?: "",
                    part.bool("required") != false,
                    part.str("status") ?: "invalid",
                )
            } ?: emptyList())
            val status = when {
                project.str("admission") == "admitted" -> "ready"
                project.bool("complete") == false -> "incomplete"
                else -> "invalid"
            }
            return Readiness(entry, status, files, diagnostics)
        }

        /** What a person reads for a role: the last segment of its kind, capitalised. */
        fun roleLabel(kind: String): String = kind.substringAfterLast('.').replaceFirstChar { it.uppercase() }

        fun grantsFrom(text: String): List<String> = text.lines().map(String::trim).filter(String::isNotEmpty)

    }
}
