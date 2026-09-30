package io.lcl.workspace.workspace

/** What the PC holds at a local document's destination, found before anything is written. */
enum class SyncState {
    ABSENT, IDENTICAL, DIFFERENT,
    /** The version this phone last synced there, unchanged since: the phone's newer text replaces it, and no one is asked. */
    SUPERSEDED,
}

/** What to do about a destination that holds different bytes; chosen by the person, never assumed. */
enum class SyncChoice { REPLACE, KEEP_PC, RENAME }

/** One local document in a sync plan: where it goes on the PC and what is there now. */
data class SyncItem(
    val id: String,
    val destination: String,
    val text: String,
    val localDigest: String,
    val state: SyncState,
    /** The PC's digest and text at the destination, when it holds something. */
    val pcDigest: String? = null,
    val pcText: String? = null,
) {
    /** The destination a RENAME writes to: the phone's version beside the PC's, never over it. */
    val renamed: String
        get() {
            val ending = listOf(LclNames.TEXT_SUFFIX, LclNames.SUFFIX).first { destination.endsWith(it) }
            return destination.removeSuffix(ending) + "-phone" + ending
        }
}

/**
 * A sync of one local project (or some of its documents) into one folder of
 * one PC project, checked but not yet done. [folders] are the project's own
 * folders, by local id, empty ones included, made on the PC too; [whole]
 * says the plan covers the project's complete inventory, which a removal
 * afterwards requires.
 */
data class SyncPlan(
    val project: ProjectInfo,
    val pcProject: ProjectInfo,
    val folder: String,
    val items: List<SyncItem>,
    val folders: List<String> = emptyList(),
    val whole: Boolean = true,
) {
    val conflicts: List<SyncItem> get() = items.filter { it.state == SyncState.DIFFERENT }

    /** Where a local id goes on the PC. */
    fun destination(id: String): String = if (folder.isEmpty()) id else "$folder/$id"
}

/** How one entry's sync ended (a folder's id ends in `/`): confirmed by the PC, or not, and why. */
data class SyncOutcome(val id: String, val destination: String, val ok: Boolean, val detail: String)

/**
 * The result of a sync: every intended document and folder confirmed, and
 * nothing changed on the phone meanwhile ([complete]), or not; whether the
 * local copy was removed afterwards, which happens only when asked for, only
 * after [complete] of a whole-project plan, and is reported only once it is
 * gone; and, when something stopped the sync or the removal, what.
 */
data class SyncResult(val outcomes: List<SyncOutcome>, val complete: Boolean, val removed: Boolean, val problem: String? = null) {
    val failures: List<SyncOutcome> get() = outcomes.filter { !it.ok }
    /** Some entries reached the PC and some did not: safe to run again, and said so. */
    val partial: Boolean get() = !complete && outcomes.any { it.ok } && outcomes.any { !it.ok }
}
