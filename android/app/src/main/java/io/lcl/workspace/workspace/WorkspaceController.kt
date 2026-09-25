package io.lcl.workspace.workspace

import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.editor.ByteSpan
import io.lcl.workspace.remote.RemoteException
import io.lcl.workspace.remote.Reply
import io.lcl.workspace.remote.arr
import io.lcl.workspace.remote.bool
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.str
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

data class ProjectInfo(val id: String, val name: String, val root: String, val isDefault: Boolean)

data class TreeEntry(val id: String, val directory: Boolean)

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
    val tree: List<TreeEntry> = emptyList(),
    val documents: List<OpenDocument> = emptyList(),
    val active: String? = null,
    /** The last Check, Validate or Inspect report, per document key, and which it was. */
    val reports: Map<String, Pair<String, JsonObject>> = emptyMap(),
    val run: RunState? = null,
    val about: JsonObject? = null,
    /** The PC's default ending for new documents. */
    val defaultEnding: String = LclNames.SUFFIX,
    val busy: Boolean = false,
) {
    val activeDocument: OpenDocument? get() = documents.firstOrNull { key(it) == active }

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
) {
    private val _ui = MutableStateFlow(WorkspaceUi())
    val ui: StateFlow<WorkspaceUi> = _ui
    private val _messages = MutableSharedFlow<String>(extraBufferCapacity = 16)
    val messages: SharedFlow<String> = _messages
    private var analysis: Job? = null
    /** The PC the workspace shows. */
    private var pcId: String? = null

    init {
        scope.launch {
            connection.state.collect { state ->
                val now = state.pcOrNull?.pcId
                if (now != pcId) {
                    // Another PC, or none: nothing of the last one stays on screen
                    // or is ever sent to the next.
                    pcId = now
                    analysis?.cancel()
                    _ui.value = WorkspaceUi()
                }
                if (state is ConnectionState.Connected) onConnected()
                else _ui.update { ui -> ui.run?.let { ui.copy(run = it.copy(connectionLost = !it.finished)) } ?: ui }
            }
        }
        scope.launch { connection.events.collect(::onEvent) }
    }

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
        val projects = reply.obj.arr("projects")?.mapNotNull { element ->
            val p = element as? JsonObject ?: return@mapNotNull null
            ProjectInfo(p.str("id") ?: return@mapNotNull null, p.str("name") ?: "", p.str("root") ?: "", p.bool("default") == true)
        } ?: emptyList()
        _ui.update { ui ->
            val kept = ui.project?.let { current -> projects.firstOrNull { it.id == current.id } }
            ui.copy(projects = projects, project = kept ?: projects.firstOrNull())
        }
    }

    fun selectProject(id: String) {
        val project = _ui.value.projects.firstOrNull { it.id == id } ?: return
        _ui.update { it.copy(project = project, tree = emptyList()) }
        scope.launch { refreshTree() }
    }

    suspend fun refreshTree() {
        val project = _ui.value.project ?: return
        val reply = ask("tree", fields(project.id)) ?: return
        if (!reply.ok) return say(reply.error ?: "The project could not be listed.")
        val entries = reply.obj.arr("entries")?.mapNotNull { element ->
            val e = element as? JsonObject ?: return@mapNotNull null
            TreeEntry(e.str("id") ?: return@mapNotNull null, e.bool("directory") == true)
        } ?: emptyList()
        _ui.update { it.copy(tree = entries) }
    }

    // --------------------------------------------------------------- documents

    fun open(id: String) {
        val project = _ui.value.project ?: return
        val key = "${project.id}/$id"
        if (_ui.value.documents.any { WorkspaceUi.key(it) == key }) {
            _ui.update { it.copy(active = key) }
            return
        }
        scope.launch {
            val reply = ask("open", fields(project.id, "document" to id)) ?: return@launch
            if (!reply.ok) return@launch say(reply.error ?: "$id could not be opened.")
            val text = reply.obj.str("text") ?: ""
            val doc = OpenDocument(project.id, id, text, text, reply.obj.str("digest") ?: "")
            _ui.update { it.copy(documents = it.documents + doc, active = key) }
            analyse(doc)
        }
    }

    fun activate(key: String) = _ui.update { it.copy(active = key) }

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

    fun create(name: String) {
        val project = _ui.value.project ?: return
        scope.launch {
            val seed = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.new\n    NAME: \"New document\"\n" +
                "    VERSION: \"1.0.0\"\n    KIND: kind.task\n    DOMAIN: \"general\"\n"
            val reply = ask("create", fields(project.id, "name" to name, "text" to seed)) ?: return@launch
            if (!reply.ok) return@launch say(reply.error ?: "Not created.")
            refreshTree()
            reply.obj.str("id")?.let(::open)
        }
    }

    // --------------------------------------------------------------------- run

    fun run(grants: RunGrants) {
        val doc = _ui.value.activeDocument ?: return
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

        fun grantsFrom(text: String): List<String> = text.lines().map(String::trim).filter(String::isNotEmpty)

    }
}
