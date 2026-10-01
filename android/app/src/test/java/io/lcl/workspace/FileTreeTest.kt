package io.lcl.workspace

import io.lcl.workspace.workspace.Explorer
import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.LoadedFolder
import io.lcl.workspace.workspace.TreeEntry
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The project tree shows what the PC listed for each unfolded folder, in its order, and nothing it
 * did not list.
 */
class FileTreeTest {
    // As the PC lists a project, one folder at a time: folders first.
    private val listings =
        mapOf(
            "" to
                LoadedFolder(
                    listOf(
                        TreeEntry("docs", true),
                        TreeEntry("empty", true),
                        TreeEntry("a.lcl", false),
                        TreeEntry("z.lcl.txt", false, "kind.project"),
                    )
                ),
            "docs" to
                LoadedFolder(listOf(TreeEntry("docs/guide", true), TreeEntry("docs/x.lcl", false))),
            "docs/guide" to LoadedFolder(listOf(TreeEntry("docs/guide/g.lcl", false))),
            "empty" to LoadedFolder(emptyList()),
        )
    private val all = Explorer(listings, setOf("docs", "docs/guide", "empty"))

    private fun shown(explorer: Explorer) = FileTree.rows(explorer).map { it.entry.id }

    @Test
    fun every_unfolded_folders_entries_show_right_after_it_with_depth_and_name() {
        val rows = FileTree.rows(all)
        assertEquals(
            listOf(
                "docs",
                "docs/guide",
                "docs/guide/g.lcl",
                "docs/x.lcl",
                "empty",
                "a.lcl",
                "z.lcl.txt",
            ),
            rows.map { it.entry.id },
        )
        assertEquals(listOf(0, 1, 2, 1, 0, 0, 0), rows.map { it.depth })
        assertEquals(
            listOf("docs", "guide", "g.lcl", "x.lcl", "empty", "a.lcl", "z.lcl.txt"),
            rows.map { it.name },
        )
        assertEquals(List(7) { false }, rows.map { it.folded })
    }

    @Test
    fun a_folder_never_listed_or_folded_shows_nothing_inside_it() {
        // Only the root listed: its folders show folded, whatever is in them.
        val root = Explorer(mapOf("" to listings.getValue("")))
        assertEquals(listOf("docs", "empty", "a.lcl", "z.lcl.txt"), shown(root))
        assertEquals(true, FileTree.rows(root).single { it.entry.id == "docs" }.folded)
        // Listed but folded: the same. Unfolded again, guide inside stays as it was.
        val folded = all.copy(expanded = all.expanded - "docs")
        assertEquals(listOf("docs", "empty", "a.lcl", "z.lcl.txt"), shown(folded))
        assertEquals(shown(all), shown(folded.copy(expanded = folded.expanded + "docs")))
        assertEquals(
            listOf("docs", "docs/guide", "docs/x.lcl", "empty", "a.lcl", "z.lcl.txt"),
            shown(all.copy(expanded = setOf("docs", "empty"))),
        )
    }

    @Test
    fun a_folder_the_pc_cut_short_gets_a_note_under_its_entries() {
        val cut =
            all.with("docs", LoadedFolder(listings.getValue("docs").entries, truncated = true))
        val rows = FileTree.rows(cut)
        val note = rows.single { it.note != null }
        assertEquals(FileTree.FOLDER_LIMITED, note.note)
        assertEquals("docs", FileTree.parentOf(note.entry.id))
        assertEquals(rows.indexOfFirst { it.entry.id == "docs/x.lcl" } + 1, rows.indexOf(note))
        assertEquals(1, rows.map { it.depth }[rows.indexOf(note)])
    }

    @Test
    fun a_folder_that_is_gone_takes_everything_below_it_with_it() {
        val without = all.without("docs")
        assertEquals(setOf("", "empty"), without.folders.keys)
        assertEquals(setOf("empty"), without.expanded)
        assertEquals(listOf("docs", "docs/guide"), FileTree.ancestors("docs/guide/g.lcl"))
        assertEquals("docs/guide", FileTree.parentOf("docs/guide/g.lcl"))
        assertEquals("", FileTree.parentOf("a.lcl"))
        assertEquals(TreeEntry("docs/x.lcl", false), all.entry("docs/x.lcl"))
        assertEquals(true, all.isFolder("docs/guide"))
        assertEquals(false, all.isFolder("docs/x.lcl"))
    }
}
