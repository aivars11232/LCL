package io.lcl.workspace.workspace

/**
 * One folder as the PC listed it: its direct children, and whether the PC's bound cut the listing
 * short.
 */
data class LoadedFolder(val entries: List<TreeEntry>, val truncated: Boolean = false)

/**
 * The project explorer as this device has read it: the folders the PC has listed so far, by their
 * id ("" is the root), and which of them are unfolded. A folder that was never unfolded was never
 * asked for.
 */
data class Explorer(
    val folders: Map<String, LoadedFolder> = emptyMap(),
    val expanded: Set<String> = emptySet(),
) {
    /** The entry the explorer has listed for [id], if its folder was read. */
    fun entry(id: String): TreeEntry? =
        folders[FileTree.parentOf(id)]?.entries?.firstOrNull { it.id == id }

    /** Whether [id] names a folder the PC has listed. */
    fun isFolder(id: String): Boolean = id.isEmpty() || entry(id)?.directory == true

    /** With [folder]'s listing replaced. */
    fun with(folder: String, loaded: LoadedFolder): Explorer =
        copy(folders = folders + (folder to loaded))

    /** Without [folder] and everything below it, unfolded or listed. */
    fun without(folder: String): Explorer =
        copy(
            folders = folders.filterKeys { it != folder && !it.startsWith("$folder/") },
            expanded =
                expanded.filterTo(mutableSetOf()) { it != folder && !it.startsWith("$folder/") },
        )
}

/** One row of the project tree: an entry the PC listed, and how it is drawn. */
data class TreeRow(
    val entry: TreeEntry,
    /** How many folders the entry is inside. */
    val depth: Int,
    /** The last part of its path. */
    val name: String,
    /** A folder whose contents are hidden. */
    val folded: Boolean,
    /** Not an entry: the note under a folder whose listing the PC cut short. */
    val note: String? = null,
)

/**
 * The project tree as folders that unfold: the rows the explorer shows, from what the PC listed for
 * each unfolded folder. The PC lists one folder at a time, folders first; nothing here invents or
 * reorders an entry.
 */
object FileTree {
    /** What the tree says under a folder whose listing the PC cut short; the PC's own words. */
    const val FOLDER_LIMITED = "Folder limited to 4096 entries."

    /** The rows [explorer] shows: each unfolded folder's entries right after it. */
    fun rows(explorer: Explorer): List<TreeRow> {
        val out = mutableListOf<TreeRow>()
        fun place(folder: String, depth: Int) {
            val loaded = explorer.folders[folder] ?: return
            for (entry in loaded.entries) {
                val folded = entry.directory && entry.id !in explorer.expanded
                out += TreeRow(entry, depth, entry.name, folded)
                if (entry.directory && !folded) place(entry.id, depth + 1)
            }
            if (loaded.truncated) {
                out +=
                    TreeRow(
                        TreeEntry("$folder/\u0000limited", false),
                        depth,
                        "",
                        false,
                        note = FOLDER_LIMITED,
                    )
            }
        }
        place("", 0)
        return out
    }

    /** The folder [id] is in: "" for the root. */
    fun parentOf(id: String): String = id.substringBeforeLast('/', "")

    /** The folders `id` is inside, outermost first. */
    fun ancestors(id: String): List<String> {
        val parts = id.split('/')
        return (1 until parts.size).map { parts.take(it).joinToString("/") }
    }
}
