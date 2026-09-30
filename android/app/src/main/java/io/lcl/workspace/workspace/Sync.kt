package io.lcl.workspace.workspace

/** What the PC holds at a local document's destination, found before anything is written. */
enum class SyncState { ABSENT, IDENTICAL, DIFFERENT }

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

/** A sync of one local project (or some of its documents) into one folder of one PC project, checked but not yet done. */
data class SyncPlan(val project: ProjectInfo, val pcProject: ProjectInfo, val folder: String, val items: List<SyncItem>) {
    val conflicts: List<SyncItem> get() = items.filter { it.state == SyncState.DIFFERENT }
}

/** How one document's sync ended: confirmed by the PC, or not, and why. */
data class SyncOutcome(val id: String, val destination: String, val ok: Boolean, val detail: String)

/**
 * The result of a sync: every intended document confirmed ([complete]), or
 * not; and whether the local copy was removed afterwards, which happens only
 * when asked for and only after [complete].
 */
data class SyncResult(val outcomes: List<SyncOutcome>, val complete: Boolean, val removed: Boolean) {
    val failures: List<SyncOutcome> get() = outcomes.filter { !it.ok }
}
