package io.lcl.workspace.workspace

/** One row of the project tree: an entry the PC listed, and how it is drawn. */
data class TreeRow(
    val entry: TreeEntry,
    /** How many folders the entry is inside. */
    val depth: Int,
    /** The last part of its path. */
    val name: String,
    /** A folder whose contents are hidden. */
    val folded: Boolean,
)

/**
 * The project tree as folders that fold: which of the PC's entries show,
 * given the folders folded. The PC lists every folder and document, parents
 * before children; nothing here invents or reorders an entry.
 */
object FileTree {
    /** The rows of [entries] that no folded folder in [folded] hides, in the PC's order. */
    fun rows(entries: List<TreeEntry>, folded: Set<String>): List<TreeRow> =
        entries.filter { entry -> ancestors(entry.id).none { it in folded } }.map { entry ->
            TreeRow(
                entry = entry,
                depth = entry.id.count { it == '/' },
                name = entry.id.substringAfterLast('/'),
                folded = entry.directory && entry.id in folded,
            )
        }

    /** The folders `id` is inside, outermost first. */
    fun ancestors(id: String): List<String> {
        val parts = id.split('/')
        return (1 until parts.size).map { parts.take(it).joinToString("/") }
    }

    /** [folded] with [folder] folded if it was open, and open if it was folded. */
    fun toggle(folded: Set<String>, folder: String): Set<String> =
        if (folder in folded) folded - folder else folded + folder

    /** [folded] with every folder around [id] opened, so [id] shows. */
    fun reveal(folded: Set<String>, id: String): Set<String> = folded - ancestors(id).toSet()
}
