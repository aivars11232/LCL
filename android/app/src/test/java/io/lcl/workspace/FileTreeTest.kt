package io.lcl.workspace

import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.TreeEntry
import org.junit.Assert.assertEquals
import org.junit.Test

/** The project tree folds folders, and shows only what the PC listed, in its order. */
class FileTreeTest {
    // As the PC lists a project: every folder and document, sorted, parents first.
    private val entries = listOf(
        TreeEntry("a.lcl", false),
        TreeEntry("docs", true),
        TreeEntry("docs/guide", true),
        TreeEntry("docs/guide/g.lcl", false),
        TreeEntry("docs/x.lcl", false),
        TreeEntry("empty", true),
        TreeEntry("z.lcl.txt", false, "kind.project"),
    )

    private fun shown(folded: Set<String>) = FileTree.rows(entries, folded).map { it.entry.id }

    @Test
    fun every_entry_shows_unfolded_with_its_depth_and_name() {
        val rows = FileTree.rows(entries, emptySet())
        assertEquals(entries, rows.map { it.entry })
        assertEquals(listOf(0, 0, 1, 2, 1, 0, 0), rows.map { it.depth })
        assertEquals(listOf("a.lcl", "docs", "guide", "g.lcl", "x.lcl", "empty", "z.lcl.txt"), rows.map { it.name })
        assertEquals(listOf(false, false, false, false, false, false, false), rows.map { it.folded })
    }

    @Test
    fun a_folded_folder_hides_everything_inside_it_and_unfolds_again() {
        val folded = FileTree.toggle(emptySet(), "docs")
        assertEquals(listOf("a.lcl", "docs", "empty", "z.lcl.txt"), shown(folded))
        assertEquals(true, FileTree.rows(entries, folded).single { it.entry.id == "docs" }.folded)
        // A folder folded inside a folded one stays folded when the outer one opens.
        val both = FileTree.toggle(folded, "docs/guide")
        assertEquals(listOf("a.lcl", "docs", "docs/guide", "docs/x.lcl", "empty", "z.lcl.txt"), shown(FileTree.toggle(both, "docs")))
        assertEquals(entries.map { it.id }, shown(FileTree.toggle(FileTree.toggle(both, "docs"), "docs/guide")))
    }

    @Test
    fun revealing_a_document_opens_only_the_folders_around_it() {
        val folded = setOf("docs", "docs/guide", "empty")
        assertEquals(setOf("empty"), FileTree.reveal(folded, "docs/guide/g.lcl"))
        assertEquals(folded, FileTree.reveal(folded, "a.lcl"))
        assertEquals(listOf("docs", "docs/guide"), FileTree.ancestors("docs/guide/g.lcl"))
    }

    @Test
    fun folds_keep_applying_to_a_newer_listing() {
        val newer = entries.filterNot { it.id == "docs/x.lcl" } + TreeEntry("docs/new.lcl", false) + TreeEntry("later", true)
        assertEquals(listOf("a.lcl", "docs", "empty", "z.lcl.txt", "later"), FileTree.rows(newer, setOf("docs")).map { it.entry.id })
        // A fold of a folder that is gone hides nothing.
        assertEquals(newer.map { it.id }, FileTree.rows(newer, setOf("gone")).map { it.entry.id })
    }
}
