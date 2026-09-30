package io.lcl.workspace

import io.lcl.workspace.local.LocalConflict
import io.lcl.workspace.local.LocalProjects
import io.lcl.workspace.local.LocalRefused
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/** Projects on the phone: made, listed lazily, edited atomically, and remembered across a restart. No PC. */
class LocalProjectsTest {
    @get:Rule
    val temp = TemporaryFolder()

    private val root by lazy { File(temp.root, "local-projects") }
    private val local by lazy { LocalProjects(root) }

    @Test
    fun a_project_its_folders_and_files_are_made_offline_and_listed_one_folder_at_a_time() {
        assertEquals(emptyList<String>(), local.projects().map { it.name })
        val alpha = local.create("Alpha")
        assertEquals("local:Alpha", alpha.id)
        assertTrue(File(root, "Alpha").isDirectory)
        assertEquals(listOf("Alpha"), local.projects().map { it.name })
        // Empty folders are folders; documents go where they are named.
        assertEquals("planning", local.mkdir("Alpha", "planning"))
        assertEquals("tasks", local.mkdir("Alpha", "tasks/"))
        local.createFile("Alpha", "main.lcl.txt", "LCL:\n")
        local.createFile("Alpha", "tasks/task_1.lcl", "LCL:\n")
        local.createFile("Alpha", "tasks/deep/inner.lcl", "LCL:\n") // folders on the way are made
        File(root, "Alpha/notes.txt").writeText("not a document")
        File(root, "Alpha/.hidden").mkdir()
        val root = local.children("Alpha", "")
        assertEquals(listOf("planning", "tasks", "main.lcl.txt"), root.entries.map { it.id })
        assertEquals(listOf(true, true, false), root.entries.map { it.directory })
        assertFalse(root.truncated)
        assertEquals(listOf("tasks/deep", "tasks/task_1.lcl"), local.children("Alpha", "tasks").entries.map { it.id })
        assertEquals(emptyList<String>(), local.children("Alpha", "planning").entries.map { it.id })
        assertEquals(listOf("main.lcl.txt", "tasks/deep/inner.lcl", "tasks/task_1.lcl"), local.documents("Alpha"))
        assertThrows(LocalRefused::class.java) { local.mkdir("Alpha", "planning") }
        assertThrows(LocalRefused::class.java) { local.mkdir("Alpha", "nowhere/deep") }
        assertThrows(LocalRefused::class.java) { local.createFile("Alpha", "main.lcl.txt", "again\n") }
        assertThrows(LocalRefused::class.java) { local.createFile("Alpha", "notes.txt", "x") }
        assertThrows(LocalRefused::class.java) { local.create("Alpha") }
    }

    @Test
    fun edits_are_saved_atomically_and_a_stale_save_is_refused_with_the_current_text() {
        local.create("Beta")
        val made = local.createFile("Beta", "a.lcl", "one\n")
        assertEquals(LocalProjects.digest("one\n"), made.digest)
        val saved = local.write("Beta", "a.lcl", "two\n", base = made.digest)
        assertEquals("two\n", local.read("Beta", "a.lcl").text)
        assertEquals(saved.digest, local.read("Beta", "a.lcl").digest)
        // No temporary file is left behind a save.
        assertEquals(listOf("a.lcl"), File(root, "Beta").list()!!.toList())
        // Written behind the app's back: a save over the old revision stops, and reports what is there.
        File(root, "Beta/a.lcl").writeText("three\n")
        val conflict = assertThrows(LocalConflict::class.java) { local.write("Beta", "a.lcl", "mine\n", base = saved.digest) }
        assertEquals("three\n", conflict.text)
        assertEquals("three\n", local.read("Beta", "a.lcl").text)
        // Without a base (a deliberate replace), it writes.
        local.write("Beta", "a.lcl", "mine\n", base = null)
        assertEquals("mine\n", local.read("Beta", "a.lcl").text)
        // Delete removes only the bytes that were confirmed.
        assertThrows(LocalRefused::class.java) { local.delete("Beta", "a.lcl", "0".repeat(64)) }
        local.delete("Beta", "a.lcl", LocalProjects.digest("mine\n"))
        assertFalse(File(root, "Beta/a.lcl").exists())
    }

    @Test
    fun everything_survives_a_restart_of_the_app() {
        local.create("Gamma")
        local.mkdir("Gamma", "empty")
        local.createFile("Gamma", "docs/x.lcl.txt", "kept\n")
        local.recordSynced("Gamma", "docs/x.lcl.txt", LocalProjects.digest("kept\n"), "p1", "Gamma/docs/x.lcl.txt", "pcdigest", 1_000L)
        // A new instance over the same storage, as after the process died.
        val again = LocalProjects(root)
        assertEquals(listOf("Gamma"), again.projects().map { it.name })
        assertEquals(listOf("docs", "empty"), again.children("Gamma", "").entries.map { it.id })
        assertEquals("kept\n", again.read("Gamma", "docs/x.lcl.txt").text)
        assertTrue(again.isSynced("Gamma", "docs/x.lcl.txt"))
        assertTrue(again.fullySynced("Gamma"))
        assertEquals("Gamma/docs/x.lcl.txt", again.syncRecords("Gamma").getValue("docs/x.lcl.txt").pcPath)
        // An edit after the sync makes it unsynced again, until a PC confirms the new bytes.
        again.write("Gamma", "docs/x.lcl.txt", "changed\n", base = null)
        assertFalse(again.isSynced("Gamma", "docs/x.lcl.txt"))
        assertFalse(again.fullySynced("Gamma"))
        // Removing the project is explicit and complete.
        again.deleteProject("Gamma")
        assertFalse(File(root, "Gamma").exists())
        assertEquals(emptyList<String>(), again.projects())
    }

    @Test
    fun nothing_reaches_outside_a_project() {
        local.create("Delta")
        File(temp.root, "secret.lcl").writeText("LCL:\n")
        for (bad in listOf("../secret.lcl", "/etc/passwd", "..", "a/../../secret.lcl", ".hidden/x.lcl", "x\\y.lcl", "")) {
            assertThrows("$bad was read", LocalRefused::class.java) { local.read("Delta", bad) }
            assertThrows("$bad was written", LocalRefused::class.java) { local.createFile("Delta", bad, "x") }
        }
        for (bad in listOf("..", "/tmp", ".git", "a/../..")) {
            assertThrows("$bad was listed", LocalRefused::class.java) { local.children("Delta", bad) }
            assertThrows("$bad was made", LocalRefused::class.java) { local.mkdir("Delta", bad) }
        }
        for (bad in listOf("", "..", "a/b", ".dot", "x\\y")) {
            assertThrows("project $bad was made", LocalRefused::class.java) { local.create(bad) }
        }
        assertTrue(File(temp.root, "secret.lcl").exists())
        assertEquals(emptyList<String>(), local.children("Delta", "").entries)
    }

    @Test
    fun a_folder_over_the_bound_says_so_and_keeps_the_first_entries() {
        local.create("Big")
        for (i in 0..LocalProjects.MAX_CHILDREN) File(root, "Big/d${i.toString().padStart(5, '0')}.lcl").writeText("")
        val listed = local.children("Big", "")
        assertEquals(LocalProjects.MAX_CHILDREN, listed.entries.size)
        assertTrue(listed.truncated)
        assertEquals("d00000.lcl", listed.entries.first().id)
    }
}
