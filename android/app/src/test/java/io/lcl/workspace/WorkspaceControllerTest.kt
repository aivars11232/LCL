package io.lcl.workspace

import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.data.MemoryStore
import io.lcl.workspace.data.PcStore
import io.lcl.workspace.editor.ByteSpan
import io.lcl.workspace.remote.Reply
import io.lcl.workspace.remote.arr
import io.lcl.workspace.remote.bool
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.objOrNull
import io.lcl.workspace.remote.sha256Hex
import io.lcl.workspace.remote.str
import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.OpenDocument
import io.lcl.workspace.workspace.RunGrants
import io.lcl.workspace.workspace.WorkspaceController
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The workspace against a scripted PC: what it asks, and what it does with the answers. */
@OptIn(ExperimentalCoroutinesApi::class)
class WorkspaceControllerTest {
    private val pc = FakePc()
    private val network = FakeNetwork()
    /** The project's files on the PC. */
    private val files = linkedMapOf("a.lcl" to "LCL:\n", "notes.lcl.txt" to "LCL:\n")
    private val kinds = mutableMapOf<String, String>()
    /** The folder whose listing the stand-in PC says its bound cut short. */
    private var truncated: String? = null
    /** Folders made with `mkdir`; the rest are implied by the files' paths. */
    private val madeFolders = mutableSetOf<String>()
    /** A PC from before `children` existed answers only `tree`. */
    private var legacyPc = false
    /** The projects the stand-in PC shares. */
    private var projects = listOf(Triple("p1", "Demo", "/home/me/demo"))

    /** The direct children of one folder, as the PC lists them: folders first. */
    private fun childrenOf(parent: String): List<JsonObject> {
        val under = { id: String ->
            if (parent.isEmpty()) id
            else if (id.startsWith("$parent/")) id.removePrefix("$parent/") else null
        }
        val folders = sortedSetOf<String>()
        val docs = sortedSetOf<String>()
        for (id in files.keys + madeFolders) {
            val rest = under(id) ?: continue
            if (rest.contains('/')) folders += rest.substringBefore('/')
            else if (id in madeFolders) folders += rest else docs += rest
        }
        val join = { name: String -> if (parent.isEmpty()) name else "$parent/$name" }
        return folders.map {
            buildJsonObject {
                put("id", join(it))
                put("name", it)
                put("directory", true)
            }
        } +
            docs.map {
                buildJsonObject {
                    put("id", join(it))
                    put("name", it)
                    put("directory", false)
                    kinds[join(it)]?.let { k -> put("kind", k) }
                }
            }
    }

    private fun isFolder(id: String) =
        id.isEmpty() ||
            childrenOf(id.substringBeforeLast('/', "")).any {
                it.str("id") == id && it.bool("directory") == true
            }

    /** The fake PC's scaffold: its bytes depend on role, mode and the file name. */
    private var templateEdition = 1

    private fun scaffoldText(role: String, mode: String, path: String) =
        "PC scaffold $templateEdition for $role ($mode) at $path\n"

    private var diagnostics: List<JsonObject> = emptyList()
    /** Holds analysis answers until released: the PC is slow. */
    private var gate: CompletableDeferred<Unit>? = null
    private var saveGate: CompletableDeferred<Unit>? = null
    private val said = mutableListOf<String>()
    private val key = "p1/a.lcl"

    private val requests
        get() = pc.sessions.flatMap { it.requests }

    private fun last(op: String) = requests.last { it.first == op }.second

    private fun digest(text: String) = sha256Hex(text.toByteArray(Charsets.UTF_8))

    init {
        pc.answer = { op, fields -> answer(op, fields) }
    }

    private suspend fun answer(op: String, f: JsonObject): Reply =
        when (op) {
            "projects" ->
                reply(
                    200,
                    "projects" to
                        JsonArray(
                            projects.map { (id, name, root) ->
                                buildJsonObject {
                                    put("id", id)
                                    put("name", name)
                                    put("root", root)
                                    put("default", id == "p1")
                                }
                            }
                        ),
                )
            "about" -> reply(200, "service" to "lcl-remote test")
            "settings" -> reply(200, "default_extension" to ".lcl")
            // The explorer: one folder's direct children, and nothing below them.
            "children" -> {
                val parent = f.str("parent") ?: ""
                when {
                    legacyPc -> reply(400, "error" to "unknown operation children")
                    !isFolder(parent) ->
                        reply(404, "error" to "there is no such folder in the project")
                    else ->
                        reply(
                            200,
                            "parent" to parent,
                            "entries" to JsonArray(childrenOf(parent)),
                            "truncated" to (truncated == parent),
                        )
                }
            }
            "mkdir" -> {
                val folder = f.str("folder")!!
                when {
                    isFolder(folder) || folder in files ->
                        reply(409, "error" to "$folder already exists")
                    !isFolder(folder.substringBeforeLast('/', "")) ->
                        reply(422, "error" to "no such parent")
                    else -> {
                        madeFolders += folder
                        reply(200, "id" to folder, "directory" to true)
                    }
                }
            }
            // As a PC before `children` lists a project: every folder and document, parents first.
            "tree" ->
                reply(
                    200,
                    "entries" to
                        JsonArray(
                            (files.keys + files.keys.flatMap(FileTree::ancestors))
                                .distinct()
                                .sorted()
                                .map {
                                    buildJsonObject {
                                        put("id", it)
                                        put("directory", it !in files)
                                        kinds[it]?.let { k -> put("kind", k) }
                                    }
                                }
                        ),
                    "truncated" to (truncated == ""),
                )
            "roles" ->
                reply(
                    200,
                    "available" to true,
                    "roles" to
                        JsonArray(
                            listOf("kind.part.rules" to "Rules", "kind.part.task" to "Task").map {
                                (r, l) ->
                                buildJsonObject {
                                    put("role", r)
                                    put("label", l)
                                }
                            }
                        ),
                )
            "scaffold" -> {
                val text = scaffoldText(f.str("role")!!, f.str("mode")!!, f.str("path")!!)
                reply(
                    200,
                    "path" to f.str("path"),
                    "role" to f.str("role"),
                    "text" to text,
                    "digest" to digest(text),
                )
            }
            "create" -> {
                val name = f.str("name")!!.let { if (it.endsWith(".lcl")) it else "$it.lcl" }
                val role = f.str("role")
                // As the PC does: a file of a role is created only from the
                // scaffold the device previewed for exactly this name.
                val text =
                    if (role != null) scaffoldText(role, f.str("mode")!!, name) else f.str("text")!!
                when {
                    role != null && f.str("scaffold_digest") == null ->
                        reply(428, "error" to "created only from a preview")
                    role != null && f.str("scaffold_digest") != digest(text) ->
                        reply(409, "error" to "the starting text changed since it was previewed")
                    else -> {
                        files[name] = text
                        role?.let { kinds[name] = it }
                        reply(
                            200,
                            "id" to name,
                            "requested" to f.str("name"),
                            "digest" to digest(text),
                        )
                    }
                }
            }
            "project" ->
                reply(
                    200,
                    "outcome" to "outcome.rejected",
                    "diagnostics" to
                        JsonArray(
                            listOf(
                                buildJsonObject {
                                    put("id", "error.project.part_missing")
                                    put("source", "main.lcl")
                                    put("position", buildJsonObject { put("line", 12) })
                                }
                            )
                        ),
                    "project" to
                        buildJsonObject {
                            put("entry", "main.lcl")
                            put("entry_status", "invalid")
                            put("complete", false)
                            put("admission", "rejected")
                            put(
                                "parts",
                                JsonArray(
                                    listOf(
                                        buildJsonObject {
                                            put("source", "task.lcl")
                                            put("unit", JsonNull)
                                            put("kind", "kind.part.task")
                                            put("required", true)
                                            put("status", "missing")
                                        },
                                        buildJsonObject {
                                            put("source", "rules.lcl")
                                            put("unit", "rules.lcl")
                                            put("kind", "kind.part.rules")
                                            put("required", true)
                                            put("status", "ready")
                                        },
                                    )
                                ),
                            )
                        },
                )
            "open" ->
                files[f.str("document")]?.let {
                    reply(200, "id" to f.str("document"), "text" to it, "digest" to digest(it))
                } ?: reply(404, "error" to "no such document")
            "save" -> save(f)
            "tokens" -> {
                gate?.await()
                // One span over the whole text it was given, named by its length.
                val length = f.str("text")!!.toByteArray().size
                reply(
                    200,
                    "tokens" to
                        JsonArray(
                            listOf(
                                buildJsonObject {
                                    put("start", 0)
                                    put("end", length)
                                    put("class", "len-$length")
                                }
                            )
                        ),
                )
            }
            "check",
            "validate",
            "inspect" -> {
                gate?.await()
                reply(
                    200,
                    "outcome" to "outcome.completed",
                    "reached" to "stage.check",
                    "diagnostics" to JsonArray(diagnostics),
                )
            }
            "run" -> reply(200, "run" to "r1")
            "answer" -> reply(200, "answered" to true)
            "follow" -> reply(200, "following" to true)
            "close" -> reply(200)
            else -> reply(400, "error" to "unknown operation $op")
        }

    /** The PC's save: only over the revision the device started from. */
    private suspend fun save(f: JsonObject): Reply {
        saveGate?.await()
        val id = f.str("document")!!
        val current = files[id] ?: return reply(404, "error" to "$id no longer exists on this PC")
        if (digest(current) != f.str("base")) {
            return reply(
                409,
                "error" to "$id changed on this PC",
                "conflict" to true,
                "text" to current,
                "digest" to digest(current),
            )
        }
        val text = f.str("text")!!
        val stored = if (text.isEmpty() || text.endsWith("\n")) text else text + "\n"
        files[id] = stored
        return reply(
            200,
            "id" to id,
            "digest" to digest(stored),
            "final_line_feed_added" to (stored != text),
        )
    }

    private fun diagnostic(source: String, start: Int, end: Int, status: String, line: Int) =
        buildJsonObject {
            put("source", source)
            put(
                "span",
                buildJsonObject {
                    put("start", start)
                    put("end", end)
                },
            )
            put(
                "position",
                buildJsonObject {
                    put("line", line)
                    put("column", 1)
                },
            )
            put("default_status", status)
        }

    private fun runEvent(name: String, data: JsonObject = JsonObject(emptyMap())) =
        buildJsonObject {
            put("type", "event")
            put("event", "run")
            put("project", "p1")
            put("run", "r1")
            put("name", name)
            put("data", data)
        }

    private fun changed(id: String, digest: String?) = buildJsonObject {
        put("type", "event")
        put("event", "document_changed")
        put("project", "p1")
        put("document", id)
        put("digest", digest)
    }

    private suspend fun TestScope.connected(): Pair<ConnectionManager, WorkspaceController> {
        val connection =
            ConnectionManager(
                PcStore(MemoryStore()),
                FakeIdentities(),
                pc,
                network,
                backgroundScope,
                discover = { emptyList() },
                now = { 1_000L },
            )
        val workspace = WorkspaceController(connection, backgroundScope, analysisDelayMs = 400)
        backgroundScope.launch { workspace.messages.collect { said += it } }
        runCurrent()
        connection.pair(pc.link(1_000L), "Pixel").getOrThrow()
        runCurrent()
        return connection to workspace
    }

    private fun WorkspaceController.doc(k: String = key): OpenDocument =
        ui.value.documents.single { "${it.project}/${it.id}" == k }

    @Test
    fun connecting_loads_the_projects_the_tree_and_the_pc_identity() = runTest {
        val (_, workspace) = connected()
        val ui = workspace.ui.value
        assertEquals("p1", ui.project!!.id)
        assertEquals(
            listOf("a.lcl", "notes.lcl.txt"),
            ui.explorer.folders[""]!!.entries.map { it.id },
        )
        // Nothing but the root was asked for: no whole-project tree.
        assertTrue(requests.none { it.first == "tree" })
        assertEquals("lcl-remote test", ui.about!!.str("service"))
        assertEquals(".lcl", ui.defaultEnding)
    }

    @Test
    fun folders_unfold_lazily_fold_and_the_document_being_edited_is_revealed() = runTest {
        files["docs/guide/g.lcl"] = "LCL:\n"
        files["docs/x.lcl"] = "LCL:\n"
        madeFolders += "planning"
        val (_, workspace) = connected()
        val shown = { FileTree.rows(workspace.ui.value.explorer).map { it.entry.id } }
        val listed = { requests.filter { it.first == "children" }.map { it.second.str("parent") } }
        // The root, folders first, every folder even an empty one; nothing below them was asked
        // for.
        assertEquals(listOf("docs", "planning", "a.lcl", "notes.lcl.txt"), shown())
        assertEquals(listOf(""), listed())
        // Unfolding docs asks for docs alone; guide inside it stays folded.
        workspace.toggleFolder("docs")
        runCurrent()
        assertEquals(
            listOf("docs", "docs/guide", "docs/x.lcl", "planning", "a.lcl", "notes.lcl.txt"),
            shown(),
        )
        assertEquals(listOf("", "docs"), listed())
        workspace.toggleFolder("docs/guide")
        runCurrent()
        assertEquals(
            listOf(
                "docs",
                "docs/guide",
                "docs/guide/g.lcl",
                "docs/x.lcl",
                "planning",
                "a.lcl",
                "notes.lcl.txt",
            ),
            shown(),
        )
        // Folding asks for nothing; unfolding again reads the folder afresh and keeps guide
        // unfolded.
        workspace.toggleFolder("docs")
        runCurrent()
        assertEquals(listOf("docs", "planning", "a.lcl", "notes.lcl.txt"), shown())
        assertEquals(listOf("", "docs", "docs/guide"), listed())
        workspace.toggleFolder("docs")
        runCurrent()
        assertEquals(7, shown().size)
        // The empty folder unfolds to nothing.
        workspace.toggleFolder("planning")
        runCurrent()
        assertEquals(7, shown().size)
        assertTrue("planning" in workspace.ui.value.explorer.expanded)
        // Opening a document inside folded folders unfolds them, and it is the one marked.
        workspace.toggleFolder("docs")
        runCurrent()
        workspace.open("docs/guide/g.lcl")
        runCurrent()
        assertTrue(setOf("docs", "docs/guide").all { it in workspace.ui.value.explorer.expanded })
        assertEquals("p1/docs/guide/g.lcl", workspace.ui.value.active)
        assertTrue(shown().contains("docs/guide/g.lcl"))
        // So does switching back to its tab.
        workspace.open("a.lcl")
        runCurrent()
        workspace.toggleFolder("docs")
        runCurrent()
        workspace.activate("p1/docs/guide/g.lcl")
        runCurrent()
        assertTrue("docs" in workspace.ui.value.explorer.expanded)
        assertTrue(requests.none { it.first == "tree" })
    }

    @Test
    fun refresh_rereads_the_root_and_unfolded_folders_and_leaves_folded_ones_alone() = runTest {
        files["docs/x.lcl"] = "LCL:\n"
        files["folded/f.lcl"] = "LCL:\n"
        val (_, workspace) = connected()
        workspace.toggleFolder("docs")
        runCurrent()
        val shown = { FileTree.rows(workspace.ui.value.explorer).map { it.entry.id } }
        // Files and folders added and removed on the PC.
        files["docs/new.lcl"] = "LCL:\n"
        files["later/y.lcl"] = "LCL:\n"
        files["folded/later.lcl"] = "LCL:\n"
        files.remove("docs/x.lcl")
        val before = requests.size
        workspace.refreshTree()
        assertEquals(
            listOf("", "docs"),
            requests.drop(before).filter { it.first == "children" }.map { it.second.str("parent") },
        )
        assertEquals(
            listOf("docs", "docs/new.lcl", "folded", "later", "a.lcl", "notes.lcl.txt"),
            shown(),
        )
        assertEquals(setOf("docs"), workspace.ui.value.explorer.expanded)
        // The folded folder is read when it is unfolded, and what was added shows then.
        workspace.toggleFolder("folded")
        runCurrent()
        assertTrue(shown().contains("folded/later.lcl"))
        // A document created from the device is listed at once, and only its folder was read.
        val beforeCreate = requests.size
        workspace.create("docs/fresh")
        runCurrent()
        assertTrue(shown().contains("docs/fresh.lcl"))
        assertTrue(
            requests
                .drop(beforeCreate)
                .filter { it.first == "children" }
                .all { it.second.str("parent") in setOf("", "docs") }
        )
        // A folder made from the device is real, empty and listed at once.
        workspace.createFolder("docs/archive")
        runCurrent()
        assertTrue(shown().contains("docs/archive"))
        assertTrue("docs/archive" in madeFolders)
        assertTrue("docs/archive" in workspace.ui.value.explorer.expanded)
        assertTrue(requests.none { it.first == "tree" })
    }

    @Test
    fun switching_projects_clears_the_last_projects_tree() = runTest {
        val (_, workspace) = connected()
        workspace.toggleFolder("docs")
        runCurrent()
        projects = projects + Triple("p2", "Other", "/home/me/other")
        workspace.loadProjects()
        workspace.selectProject("p2")
        runCurrent()
        assertEquals("p2", workspace.ui.value.project?.id)
        assertTrue(workspace.ui.value.explorers["p2"]?.folders?.containsKey("") == true)
        assertFalse(
            workspace.ui.value.explorers.containsKey("p1") &&
                workspace.ui.value.explorer.folders.containsKey("docs")
        )
        // Back to p1: its explorer starts over too.
        workspace.selectProject("p1")
        runCurrent()
        assertEquals(setOf(""), workspace.ui.value.explorer.folders.keys)
        assertEquals(emptySet<String>(), workspace.ui.value.explorer.expanded)
    }

    @Test
    fun a_pc_from_before_children_is_read_through_its_whole_tree() = runTest {
        legacyPc = true
        files["docs/x.lcl"] = "LCL:\n"
        val (_, workspace) = connected()
        val shown = { FileTree.rows(workspace.ui.value.explorer).map { it.entry.id } }
        assertEquals(listOf("docs", "a.lcl", "notes.lcl.txt"), shown())
        workspace.toggleFolder("docs")
        runCurrent()
        assertEquals(listOf("docs", "docs/x.lcl", "a.lcl", "notes.lcl.txt"), shown())
        assertTrue(requests.any { it.first == "tree" })
    }

    @Test
    fun a_folder_the_pc_cut_short_says_so_under_that_folder_alone() = runTest {
        files["big/b.lcl"] = "LCL:\n"
        val (_, workspace) = connected()
        val notes = {
            FileTree.rows(workspace.ui.value.explorer)
                .filter { it.note != null }
                .map { FileTree.parentOf(it.entry.id) }
        }
        assertEquals(emptyList<String>(), notes())
        truncated = "big"
        workspace.toggleFolder("big")
        runCurrent()
        assertEquals(listOf("big"), notes())
        assertTrue(workspace.ui.value.explorer.folders["big"]!!.truncated)
        assertFalse(workspace.ui.value.explorer.folders[""]!!.truncated)
        // Files already listed still open.
        workspace.open("big/b.lcl")
        runCurrent()
        assertEquals("big/b.lcl", workspace.ui.value.activeDocument?.id)
        truncated = null
        workspace.refreshTree()
        assertEquals(emptyList<String>(), notes())
    }

    @Test
    fun the_folder_limit_note_says_what_the_pc_workspace_says() {
        val page = java.io.File("../../impl/crates/lcl-workspace/assets/app.js").readText()
        assertTrue(page.contains("const FOLDER_LIMITED = \"${FileTree.FOLDER_LIMITED}\";"))
    }

    @Test
    fun new_by_role_asks_the_pc_for_its_scaffold_and_sends_no_text() = runTest {
        val (_, workspace) = connected()
        assertEquals(
            listOf("kind.part.rules", "kind.part.task"),
            workspace.ui.value.roles.map { it.role },
        )
        // The preview is the PC's own text for exactly the name it will create.
        var preview: io.lcl.workspace.workspace.ScaffoldPreview? = null
        backgroundScope.launch {
            preview = workspace.previewScaffold("house", "kind.part.rules", "minimal")
        }
        runCurrent()
        assertEquals("house.lcl", last("scaffold").str("path"))
        assertEquals("PC scaffold 1 for kind.part.rules (minimal) at house.lcl\n", preview!!.text)
        workspace.create("house", "kind.part.rules", "minimal", preview!!.digest)
        runCurrent()
        val sent = last("create")
        assertEquals("kind.part.rules", sent.str("role"))
        assertEquals("minimal", sent.str("mode"))
        assertEquals(preview!!.digest, sent.str("scaffold_digest"))
        assertEquals("a file made by role carries no text from the phone", null, sent["text"])
        assertEquals(preview!!.text, files["house.lcl"])
        assertEquals("kind.part.rules", workspace.ui.value.explorer.entry("house.lcl")!!.kind)
        // Blank stays available and needs no preview.
        workspace.create("blank")
        runCurrent()
        assertEquals(null, last("create").str("role"))
        assertTrue(files.containsKey("blank.lcl"))
    }

    @Test
    fun a_role_file_is_not_created_when_the_pc_text_changed_since_the_preview() = runTest {
        val (_, workspace) = connected()
        var preview: io.lcl.workspace.workspace.ScaffoldPreview? = null
        backgroundScope.launch {
            preview = workspace.previewScaffold("rules", "kind.part.rules", "guided")
        }
        runCurrent()
        // A template changes on the PC after the preview: nothing is created.
        templateEdition = 2
        workspace.create("rules", "kind.part.rules", "guided", preview!!.digest)
        runCurrent()
        assertFalse(files.containsKey("rules.lcl"))
        assertTrue(said.last(), said.last().contains("changed since it was previewed"))
        // Without a preview the PC refuses too.
        workspace.create("rules", "kind.part.rules", "guided", null)
        runCurrent()
        assertFalse(files.containsKey("rules.lcl"))
        assertEquals(null, last("create").str("scaffold_digest"))
    }

    @Test
    fun readiness_is_the_pc_engines_record() = runTest {
        val (_, workspace) = connected()
        workspace.readiness("main.lcl")
        runCurrent()
        assertEquals("main.lcl", last("project").str("entry"))
        val readiness = workspace.ui.value.readiness!!
        assertEquals("incomplete", readiness.status)
        assertEquals(
            listOf("main.lcl" to "invalid", "task.lcl" to "missing", "rules.lcl" to "ready"),
            readiness.files.map { it.path to it.status },
        )
        assertEquals(null, readiness.files[1].unit)
        assertEquals(listOf("error.project.part_missing — main.lcl:12"), readiness.diagnostics)
        workspace.dismissReadiness()
        assertEquals(null, workspace.ui.value.readiness)
        val admitted =
            WorkspaceController.readinessOf(
                "m.lcl",
                buildJsonObject {
                    put(
                        "project",
                        buildJsonObject {
                            put("entry", "m.lcl")
                            put("entry_status", "ready")
                            put("complete", true)
                            put("admission", "admitted")
                            put("parts", JsonArray(emptyList()))
                        },
                    )
                },
            )
        assertEquals("ready", admitted.status)
        assertEquals(
            "invalid",
            WorkspaceController.readinessOf("x.lcl", JsonObject(emptyMap())).status,
        )
    }

    @Test
    fun typing_shows_at_once_and_the_pc_is_asked_about_the_latest_text_only() = runTest {
        val (_, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        assertEquals("LCL:\n", workspace.doc().text)
        assertEquals(listOf(ByteSpan(0, 5, "len-5")), workspace.doc().tokens)
        gate = CompletableDeferred()
        workspace.edit(key, "LCL:\n    X\n")
        assertEquals("the edit waited for the PC", "LCL:\n    X\n", workspace.doc().text)
        advanceTimeBy(401)
        runCurrent() // asked about revision 1; the answer is held
        workspace.edit(key, "LCL:\n    XY\n") // typing goes on, superseding that question
        gate!!.complete(Unit)
        runCurrent()
        assertEquals(listOf(ByteSpan(0, 5, "len-5")), workspace.doc().tokens)
        advanceTimeBy(401)
        runCurrent()
        assertEquals(listOf(ByteSpan(0, 12, "len-12")), workspace.doc().tokens)
        assertTrue(said.toString(), said.none { "did not answer" in it })
        val asked = requests.filter { it.first == "tokens" }.map { it.second.str("text") }
        assertEquals(listOf("LCL:\n", "LCL:\n    X\n", "LCL:\n    XY\n"), asked)
    }

    @Test
    fun a_check_marks_diagnostics_only_on_the_text_it_checked() = runTest {
        diagnostics =
            listOf(
                diagnostic("a.lcl", 0, 3, "status.invalid", 1),
                diagnostic("other.lcl", 0, 1, "status.invalid", 1),
            )
        val (_, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        workspace.analyse("check")
        runCurrent()
        assertEquals(listOf(ByteSpan(0, 3, "bad", 1)), workspace.doc().marks)
        assertEquals("Check", workspace.ui.value.reports.getValue(key).first)
        // An answer that arrives after the text moved on is not drawn on it.
        diagnostics = listOf(diagnostic("a.lcl", 0, 3, "status.blocked", 1))
        gate = CompletableDeferred()
        workspace.analyse("check")
        runCurrent()
        workspace.edit(key, "LCL:\n\n")
        gate!!.complete(Unit)
        runCurrent()
        assertTrue(
            workspace.doc().marks.toString(),
            workspace.doc().marks.none { it.kind == "warn" },
        )
        val shown = workspace.ui.value.reports.getValue(key).second
        assertEquals(
            "a report for older text replaced the current one",
            "status.invalid",
            shown.arr("diagnostics")!!.first().objOrNull!!.str("default_status"),
        )
    }

    @Test
    fun a_save_names_its_base_and_a_conflict_loses_nothing() = runTest {
        val (_, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        workspace.edit(key, "LCL:\nmine\n")
        workspace.save(key)
        runCurrent()
        assertEquals(digest("LCL:\n"), last("save").str("base"))
        assertEquals("LCL:\nmine\n", files["a.lcl"])
        assertFalse(workspace.doc().dirty)
        assertEquals(digest("LCL:\nmine\n"), workspace.doc().base)

        // Someone saves on the PC; this device's next save is refused.
        files["a.lcl"] = "LCL:\ntheirs\n"
        workspace.edit(key, "LCL:\nmine again\n")
        workspace.save(key)
        runCurrent()
        assertEquals("the PC's newer text was overwritten", "LCL:\ntheirs\n", files["a.lcl"])
        assertEquals("LCL:\nmine again\n", workspace.doc().text)
        assertEquals("LCL:\ntheirs\n", workspace.doc().conflict!!.text)
        assertTrue(said.last(), said.last().contains("changed on the PC"))

        // Keeping mine is the person's decision, and replaces exactly the revision they saw.
        workspace.keepMine(key)
        workspace.save(key)
        runCurrent()
        assertEquals(digest("LCL:\ntheirs\n"), last("save").str("base"))
        assertEquals("LCL:\nmine again\n", files["a.lcl"])
        assertNull(workspace.doc().conflict)
        assertFalse(workspace.doc().dirty)
    }

    @Test
    fun typing_during_a_save_stays_unsaved() = runTest {
        val (_, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        workspace.edit(key, "LCL:\nA")
        saveGate = CompletableDeferred()
        workspace.save(key)
        runCurrent()
        workspace.edit(key, "LCL:\nAB\n")
        saveGate!!.complete(Unit)
        runCurrent()
        val doc = workspace.doc()
        assertEquals("LCL:\nA\n", files["a.lcl"])
        assertEquals("LCL:\nA\n", doc.saved)
        assertEquals("LCL:\nAB\n", doc.text)
        assertTrue(doc.dirty)
        assertEquals(digest("LCL:\nA\n"), doc.base)
    }

    @Test
    fun a_final_line_feed_the_pc_adds_is_shown() = runTest {
        val (_, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        workspace.edit(key, "LCL:\nno newline")
        workspace.save(key)
        runCurrent()
        assertEquals("LCL:\nno newline\n", workspace.doc().text)
        assertFalse(workspace.doc().dirty)
        assertTrue(said.last().contains("final line feed"))
    }

    @Test
    fun changes_on_the_pc_refresh_a_clean_copy_and_stop_a_dirty_one() = runTest {
        val (_, workspace) = connected()
        workspace.open("a.lcl")
        workspace.open("notes.lcl.txt")
        runCurrent()
        val session = pc.sessions.last()

        files["a.lcl"] = "LCL:\nfrom the PC\n"
        session.emit(changed("a.lcl", digest(files.getValue("a.lcl"))))
        runCurrent()
        assertEquals("LCL:\nfrom the PC\n", workspace.doc().text)
        assertFalse(workspace.doc().dirty)

        val notes = "p1/notes.lcl.txt"
        workspace.edit(notes, "LCL:\nmine\n")
        files["notes.lcl.txt"] = "LCL:\ntheirs\n"
        session.emit(changed("notes.lcl.txt", digest("LCL:\ntheirs\n")))
        runCurrent()
        assertEquals("LCL:\nmine\n", workspace.doc(notes).text)
        assertEquals("LCL:\ntheirs\n", workspace.doc(notes).conflict!!.text)

        files.remove("a.lcl")
        session.emit(changed("a.lcl", null))
        runCurrent()
        assertTrue(workspace.doc().deletedOnPc)
        assertEquals("LCL:\nfrom the PC\n", workspace.doc().text)
    }

    @Test
    fun edits_made_on_the_pc_while_away_are_found_on_reconnect() = runTest {
        val (_, workspace) = connected()
        workspace.open("a.lcl")
        workspace.open("notes.lcl.txt")
        runCurrent()
        workspace.edit("p1/notes.lcl.txt", "LCL:\nmine\n")
        pc.sessions.last().drop()
        runCurrent()
        files["a.lcl"] = "LCL:\nchanged while away\n"
        files["notes.lcl.txt"] = "LCL:\nalso changed\n"
        advanceTimeBy(1_001)
        runCurrent()
        assertEquals("LCL:\nchanged while away\n", workspace.doc().text)
        val notes = workspace.doc("p1/notes.lcl.txt")
        assertEquals("LCL:\nmine\n", notes.text)
        assertEquals("LCL:\nalso changed\n", notes.conflict!!.text)
    }

    @Test
    fun a_run_waits_for_this_device_at_every_effect_and_survives_a_reconnect() = runTest {
        val (connection, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        workspace.run(RunGrants(write = listOf("out/report.txt")))
        runCurrent()
        val started = last("run")
        assertEquals(true, started.bool("break_effects"))
        assertEquals(
            listOf("out/report.txt"),
            started.obj("grants")!!.arr("write")!!.map { it.jsonPrimitive.content },
        )
        assertEquals("LCL:\n", started.str("text"))

        val session = pc.sessions.last()
        session.emit(runEvent("operation", buildJsonObject { put("operation", "core.read") }))
        session.emit(
            runEvent(
                "paused",
                buildJsonObject {
                    put("sequence", 3)
                    put("effect", "effect.file.write")
                },
            )
        )
        runCurrent()
        assertEquals(3L, workspace.ui.value.run!!.paused!!.long("sequence"))
        assertTrue(
            "an effect was approved without the person",
            requests.none { it.first == "answer" },
        )

        workspace.answer("continue")
        runCurrent()
        assertEquals("continue", last("answer").str("answer"))
        assertEquals(3L, last("answer").long("sequence"))
        assertNull(workspace.ui.value.run!!.paused)

        // The connection drops mid-run and comes back.
        session.drop()
        runCurrent()
        assertTrue(workspace.ui.value.run!!.connectionLost)
        advanceTimeBy(1_001)
        runCurrent()
        assertTrue(connection.state.value is ConnectionState.Connected)
        assertEquals("r1", last("follow").str("run"))
        assertEquals(2L, last("follow").long("from"))
        assertFalse(workspace.ui.value.run!!.connectionLost)

        val again = pc.sessions.last()
        again.emit(runEvent("paused", buildJsonObject { put("sequence", 5) }))
        runCurrent()
        workspace.answer("deny")
        runCurrent()
        assertEquals("deny", last("answer").str("answer"))
        diagnostics = listOf(diagnostic("a.lcl", 0, 3, "status.failed", 1))
        again.emit(
            runEvent(
                "report",
                buildJsonObject {
                    put("outcome", "outcome.completed")
                    put("diagnostics", JsonArray(diagnostics))
                },
            )
        )
        again.emit(runEvent("end"))
        runCurrent()
        val run = workspace.ui.value.run!!
        assertTrue(run.finished)
        assertEquals("Run", workspace.ui.value.reports.getValue(key).first)
        assertEquals(listOf(ByteSpan(0, 3, "bad", 1)), workspace.doc().marks)
    }

    @Test
    fun forgetting_the_pc_clears_everything_it_showed() = runTest {
        val (connection, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        assertEquals(1, workspace.ui.value.documents.size)
        connection.forget("pc1")
        runCurrent()
        val ui = workspace.ui.value
        assertTrue(ui.documents.isEmpty())
        assertNull(ui.project)
        assertTrue(ui.explorers.isEmpty())
        assertNull(ui.about)
    }

    @Test
    fun without_a_connection_nothing_is_sent_and_the_person_is_told() = runTest {
        val (connection, workspace) = connected()
        workspace.open("a.lcl")
        runCurrent()
        connection.disconnect()
        runCurrent()
        val before = requests.size
        workspace.edit(key, "LCL:\noffline edit\n")
        workspace.save(key)
        runCurrent()
        assertEquals(before, requests.size)
        assertEquals("LCL:\noffline edit\n", workspace.doc().text)
        assertTrue(workspace.doc().dirty)
        assertTrue(said.last(), said.last().contains("Not connected"))
    }
}
