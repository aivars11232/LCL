package io.lcl.workspace

import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.data.MemoryStore
import io.lcl.workspace.data.PcStore
import io.lcl.workspace.local.LocalProjects
import io.lcl.workspace.remote.Reply
import io.lcl.workspace.remote.str
import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.ProjectInfo
import io.lcl.workspace.workspace.SyncChoice
import io.lcl.workspace.workspace.SyncPlan
import io.lcl.workspace.workspace.SyncState
import io.lcl.workspace.workspace.WorkspaceController
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.io.IOException

/**
 * Projects on the phone, with and without a PC: made, edited and saved
 * offline, kept across a restart, listed beside the PC's once connected, and
 * synced to the PC only on request, with every conflict decided by the person
 * and every document confirmed by the PC before it counts as synced.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class LocalWorkspaceTest {
    @get:Rule
    val temp = TemporaryFolder()

    private val pc = FakePc()
    private val network = FakeNetwork()
    /** The PC project's files, as the fake PC holds them, by path. */
    private val pcFiles = linkedMapOf<String, String>()
    /** Folders the fake PC was asked to make. */
    private val pcFolders = mutableListOf<String>()
    private val said = mutableListOf<String>()
    /** The digest the fake PC reports for a written file; a wrong one is a PC that did not keep the bytes. */
    private var reportedDigest: (String) -> String = { LocalProjects.digest(it) }
    /** After this many writes the PC goes away. */
    private var writesBeforeLoss = Int.MAX_VALUE
    private val local by lazy { LocalProjects(File(temp.root, "local-projects")) }

    init {
        pc.answer = { op, f -> answer(op, f) }
    }

    private fun digest(text: String) = LocalProjects.digest(text)

    private suspend fun answer(op: String, f: JsonObject): Reply = when (op) {
        "projects" -> reply(200, "projects" to JsonArray(listOf(buildJsonObject { put("id", "p1"); put("name", "Demo"); put("root", "/home/me/demo"); put("default", true) })))
        "about" -> reply(200, "service" to "lcl-remote test")
        "settings" -> reply(200, "default_extension" to ".lcl")
        "roles" -> reply(200, "available" to false, "roles" to JsonArray(emptyList()))
        "children" -> reply(200, "parent" to (f.str("parent") ?: ""), "entries" to JsonArray(emptyList()), "truncated" to false)
        "open" -> {
            val id = f.str("document")!!
            pcFiles[id]?.let { reply(200, "id" to id, "text" to it, "digest" to digest(it)) } ?: reply(404, "error" to "$id does not exist")
        }
        "mkdir" -> {
            val folder = f.str("folder")!!
            if (folder in pcFolders) reply(409, "error" to "$folder already exists") else { pcFolders += folder; reply(200, "id" to folder, "directory" to true) }
        }
        "create" -> {
            if (--writesBeforeLoss < 0) throw IOException("dropped")
            val name = f.str("name")!!
            if (name in pcFiles) reply(409, "error" to "$name already exists") else {
                val text = f.str("text")!!
                pcFiles[name] = text
                reply(200, "id" to name, "requested" to name, "digest" to reportedDigest(text))
            }
        }
        "save" -> {
            if (--writesBeforeLoss < 0) throw IOException("dropped")
            val id = f.str("document")!!
            val current = pcFiles[id]
            when {
                current == null -> reply(404, "error" to "$id does not exist")
                digest(current) != f.str("base") -> reply(409, "error" to "$id changed", "conflict" to true, "text" to current, "digest" to digest(current))
                else -> { val text = f.str("text")!!; pcFiles[id] = text; reply(200, "id" to id, "digest" to reportedDigest(text), "final_line_feed_added" to false) }
            }
        }
        else -> reply(400, "error" to "unknown operation $op")
    }

    private fun TestScope.controller(): Pair<ConnectionManager, WorkspaceController> {
        val connection = ConnectionManager(PcStore(MemoryStore()), FakeIdentities(), pc, network, backgroundScope, discover = { emptyList() }, now = { 1_000L })
        val workspace = WorkspaceController(connection, backgroundScope, analysisDelayMs = 400, local = local, io = Dispatchers.Unconfined, now = { 42L })
        backgroundScope.launch { workspace.messages.collect { said += it } }
        runCurrent()
        return connection to workspace
    }

    private suspend fun TestScope.connected(): Pair<ConnectionManager, WorkspaceController> {
        val (connection, workspace) = controller()
        connection.pair(pc.link(1_000L), "Pixel").getOrThrow()
        runCurrent()
        return connection to workspace
    }

    private fun WorkspaceController.shown() = FileTree.rows(ui.value.explorer).map { it.entry.id }

    @Test
    fun a_project_is_made_edited_and_saved_with_no_pc_and_is_there_after_a_restart() = runTest {
        val (_, workspace) = controller()
        assertEquals(emptyList<ProjectInfo>(), workspace.ui.value.projects)
        workspace.createLocalProject("Alpha")
        runCurrent()
        val project = workspace.ui.value.project!!
        assertEquals("local:Alpha", project.id)
        assertTrue(project.local)
        assertEquals(listOf("local:Alpha"), workspace.ui.value.projects.map { it.id })
        // The explorer works the same: lazy, empty folders shown, no PC asked.
        workspace.createFolder("planning")
        runCurrent()
        assertEquals(listOf("planning"), workspace.shown())
        workspace.create("main")
        runCurrent()
        assertEquals(listOf("planning", "main.lcl"), workspace.shown())
        assertEquals("local:Alpha/main.lcl", workspace.ui.value.active)
        workspace.create("planning/phase_1.lcl.txt")
        runCurrent()
        assertEquals(listOf("planning", "planning/phase_1.lcl.txt", "main.lcl"), workspace.shown())
        // Edit and save, offline.
        val key = "local:Alpha/main.lcl"
        workspace.activate(key)
        workspace.edit(key, "LCL:\n    VERSION: \"0.1.0\"\nedited\n")
        assertTrue(workspace.ui.value.activeDocument!!.dirty)
        workspace.save(key)
        runCurrent()
        assertFalse(workspace.ui.value.activeDocument!!.dirty)
        assertEquals("LCL:\n    VERSION: \"0.1.0\"\nedited\n", local.read("Alpha", "main.lcl").text)
        // No engine on the phone: analysis and runs say so, and nothing is sent.
        workspace.analyse("check")
        workspace.run(io.lcl.workspace.workspace.RunGrants())
        runCurrent()
        assertTrue(said.any { it.startsWith("Check needs the PC") })
        assertTrue(said.any { it.startsWith("Run needs the PC") })
        assertTrue(pc.sessions.isEmpty())
        assertNull(workspace.ui.value.activeDocument!!.tokens)
        // Reload reads the phone's copy; delete removes it.
        workspace.edit(key, "typed but not saved\n")
        workspace.reload(key)
        runCurrent()
        assertEquals("LCL:\n    VERSION: \"0.1.0\"\nedited\n", workspace.ui.value.activeDocument!!.text)
        workspace.deleteLocal("local:Alpha/planning/phase_1.lcl.txt".let { k -> workspace.activate(k); k })
        runCurrent()
        assertFalse(File(temp.root, "local-projects/Alpha/planning/phase_1.lcl.txt").exists())
        assertEquals(listOf("planning", "main.lcl"), workspace.shown())
        // A restart: another controller over the same storage finds it all.
        val (_, restarted) = controller()
        assertEquals(listOf("local:Alpha"), restarted.ui.value.projects.map { it.id })
        restarted.selectProject("local:Alpha")
        runCurrent()
        assertEquals(listOf("planning", "main.lcl"), restarted.shown())
        restarted.open("main.lcl")
        runCurrent()
        assertEquals("LCL:\n    VERSION: \"0.1.0\"\nedited\n", restarted.ui.value.activeDocument!!.text)
    }

    @Test
    fun connecting_later_lists_the_pc_beside_the_phone_and_leaving_the_pc_keeps_the_phone() = runTest {
        local.create("Alpha")
        local.createFile("Alpha", "main.lcl", "LCL:\n")
        val (connection, workspace) = controller()
        workspace.selectProject("local:Alpha")
        workspace.open("main.lcl")
        runCurrent()
        workspace.edit("local:Alpha/main.lcl", "LCL:\nunsaved\n")
        connection.pair(pc.link(1_000L), "Pixel").getOrThrow()
        runCurrent()
        assertEquals(listOf("p1", "local:Alpha"), workspace.ui.value.projects.map { it.id })
        // The local project stayed chosen, its unsaved edit untouched, and nothing was synced by connecting.
        assertEquals("local:Alpha", workspace.ui.value.project!!.id)
        assertEquals("LCL:\nunsaved\n", workspace.ui.value.activeDocument!!.text)
        assertTrue(pcFiles.isEmpty())
        assertTrue(pc.sessions.flatMap { it.requests }.none { it.first in setOf("create", "save", "mkdir") })
        // The PC's project can be worked in, and the phone's is still there.
        workspace.selectProject("p1")
        runCurrent()
        assertFalse(workspace.ui.value.project!!.local)
        connection.forget("pc1")
        runCurrent()
        assertEquals(listOf("local:Alpha"), workspace.ui.value.projects.map { it.id })
        assertEquals("LCL:\nunsaved\n", workspace.ui.value.documents.single().text)
    }

    private suspend fun TestScope.planned(workspace: WorkspaceController, folder: String = "Alpha", ids: List<String>? = null): SyncPlan {
        val alpha = workspace.ui.value.projects.first { it.id == "local:Alpha" }
        val p1 = workspace.ui.value.projects.first { it.id == "p1" }
        return workspace.planSync(alpha, p1, folder, ids).getOrThrow()
    }

    @Test
    fun a_sync_is_planned_against_the_pc_and_writes_only_what_the_person_chose() = runTest {
        local.create("Alpha")
        local.createFile("Alpha", "main.lcl", "LCL:\nmain\n")
        local.createFile("Alpha", "rules/security.lcl.txt", "LCL:\nrules\n")
        local.createFile("Alpha", "same.lcl", "LCL:\nsame\n")
        local.createFile("Alpha", "differs.lcl", "LCL:\nphone\n")
        pcFiles["Alpha/same.lcl"] = "LCL:\nsame\n"
        pcFiles["Alpha/differs.lcl"] = "LCL:\npc\n"
        val (_, workspace) = connected()
        // Offline, a plan is refused before anything is read from the PC.
        val plan = planned(workspace)
        assertEquals(
            mapOf("differs.lcl" to SyncState.DIFFERENT, "main.lcl" to SyncState.ABSENT, "rules/security.lcl.txt" to SyncState.ABSENT, "same.lcl" to SyncState.IDENTICAL),
            plan.items.associate { it.id to it.state },
        )
        assertEquals("Alpha/rules/security.lcl.txt", plan.items.first { it.id == "rules/security.lcl.txt" }.destination)
        assertEquals("LCL:\npc\n", plan.conflicts.single().pcText)
        // Planning wrote nothing.
        assertEquals(setOf("Alpha/same.lcl", "Alpha/differs.lcl"), pcFiles.keys)
        assertTrue(pcFolders.isEmpty())
        // Cancelled: nothing more happens. Then done, keeping the PC's version of the conflict.
        val result = workspace.runSync(plan, mapOf("differs.lcl" to SyncChoice.KEEP_PC), removeAfter = false)
        assertEquals(listOf("Alpha", "Alpha/rules"), pcFolders)
        assertEquals("LCL:\nmain\n", pcFiles["Alpha/main.lcl"])
        assertEquals("LCL:\nrules\n", pcFiles["Alpha/rules/security.lcl.txt"])
        assertEquals("the PC's version was overwritten", "LCL:\npc\n", pcFiles["Alpha/differs.lcl"])
        assertFalse(result.complete)
        assertEquals(listOf("differs.lcl"), result.failures.map { it.id })
        assertTrue(local.isSynced("Alpha", "main.lcl"))
        assertTrue(local.isSynced("Alpha", "same.lcl"))
        assertFalse(local.isSynced("Alpha", "differs.lcl"))
        assertFalse(local.fullySynced("Alpha"))
        // Replace and rename resolve the conflict, each confirmed by the PC.
        val again = planned(workspace, ids = listOf("differs.lcl"))
        val renamed = workspace.runSync(again, mapOf("differs.lcl" to SyncChoice.RENAME), removeAfter = false)
        assertTrue(renamed.complete)
        assertEquals("LCL:\nphone\n", pcFiles["Alpha/differs-phone.lcl"])
        assertEquals("LCL:\npc\n", pcFiles["Alpha/differs.lcl"])
        assertEquals("Alpha/differs-phone.lcl", local.syncRecords("Alpha").getValue("differs.lcl").pcPath)
        local.write("Alpha", "differs.lcl", "LCL:\nphone 2\n", base = null)
        val replaced = workspace.runSync(planned(workspace, ids = listOf("differs.lcl")), mapOf("differs.lcl" to SyncChoice.REPLACE), removeAfter = false)
        assertTrue(replaced.complete)
        assertEquals("LCL:\nphone 2\n", pcFiles["Alpha/differs.lcl"])
        assertTrue(local.fullySynced("Alpha"))
        assertTrue(workspace.localProjectSynced("local:Alpha"))
    }

    @Test
    fun a_pc_that_did_not_keep_the_bytes_and_a_lost_connection_leave_the_phone_unchanged_and_unsynced() = runTest {
        local.create("Alpha")
        local.createFile("Alpha", "a.lcl", "LCL:\na\n")
        local.createFile("Alpha", "b.lcl", "LCL:\nb\n")
        local.createFile("Alpha", "c.lcl", "LCL:\nc\n")
        val (_, workspace) = connected()
        // The PC answers with another digest: not confirmed, not synced.
        reportedDigest = { "0".repeat(64) }
        val wrong = workspace.runSync(planned(workspace, ids = listOf("a.lcl")), emptyMap(), removeAfter = true)
        assertFalse(wrong.complete)
        assertFalse(wrong.removed)
        assertTrue(wrong.failures.single().detail.contains("other bytes"))
        assertFalse(local.isSynced("Alpha", "a.lcl"))
        assertTrue(File(temp.root, "local-projects/Alpha/a.lcl").exists())
        reportedDigest = { digest(it) }
        // The connection drops after the first write: the rest is not attempted, nothing local goes.
        pcFiles.remove("Alpha/a.lcl")
        writesBeforeLoss = 1
        val lost = workspace.runSync(planned(workspace), emptyMap(), removeAfter = true)
        assertFalse(lost.complete)
        assertFalse(lost.removed)
        assertTrue(lost.outcomes.first { it.id == "a.lcl" }.ok)
        assertTrue(lost.outcomes.filter { it.id != "a.lcl" }.all { !it.ok && it.detail.startsWith("not") })
        assertTrue(local.isSynced("Alpha", "a.lcl"))
        assertFalse(local.isSynced("Alpha", "b.lcl"))
        assertTrue(File(temp.root, "local-projects/Alpha").isDirectory)
        assertEquals(3, local.documents("Alpha").size)
        // Remove is refused while anything is unsynced.
        workspace.removeLocalProject("local:Alpha")
        runCurrent()
        assertTrue(File(temp.root, "local-projects/Alpha").isDirectory)
        assertTrue(said.any { it.contains("not fully synced") })
    }

    @Test
    fun a_whole_project_syncs_with_its_structure_and_then_may_be_removed_from_the_phone() = runTest {
        local.create("Alpha")
        local.mkdir("Alpha", "empty")
        local.createFile("Alpha", "main.lcl.txt", "LCL:\nmain")
        local.createFile("Alpha", "tasks/deep/inner.lcl", "LCL:\ninner\n")
        val (_, workspace) = connected()
        workspace.selectProject("local:Alpha")
        workspace.open("main.lcl.txt")
        runCurrent()
        val result = workspace.runSync(planned(workspace, folder = "phone/Alpha"), emptyMap(), removeAfter = true)
        runCurrent()
        assertTrue(result.complete)
        assertTrue(result.removed)
        assertEquals(listOf("phone", "phone/Alpha", "phone/Alpha/tasks", "phone/Alpha/tasks/deep"), pcFolders)
        assertEquals("LCL:\nmain", pcFiles["phone/Alpha/main.lcl.txt"])
        assertEquals("LCL:\ninner\n", pcFiles["phone/Alpha/tasks/deep/inner.lcl"])
        // Gone from the phone, from the list, and from the tabs, only now.
        assertFalse(File(temp.root, "local-projects/Alpha").exists())
        assertEquals(listOf("p1"), workspace.ui.value.projects.map { it.id })
        assertTrue(workspace.ui.value.documents.isEmpty())
        assertNull(workspace.ui.value.explorers["local:Alpha"])
    }
}
