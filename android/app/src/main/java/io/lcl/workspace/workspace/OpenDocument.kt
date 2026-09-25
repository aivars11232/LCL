package io.lcl.workspace.workspace

import io.lcl.workspace.editor.ByteSpan
import io.lcl.workspace.editor.Utf8Index
import io.lcl.workspace.editor.carry
import io.lcl.workspace.editor.editBetween

/**
 * One document open on this device, and where it stands against the PC.
 *
 * The PC owns the file. This is a copy with a record of which revision of the
 * file it started from — [base], the SHA-256 of the bytes on the PC — so a
 * save can say "replace exactly this revision" and the PC can refuse it when
 * the file has moved on. Nothing here ever decides to overwrite newer content:
 * a conflict stops, and only the person resolves it.
 */
data class OpenDocument(
    val project: String,
    val id: String,
    /** What is on screen. Always shown, whatever the analysis state. */
    val text: String,
    /** The text as last loaded from or saved to the PC. */
    val saved: String,
    /** The PC revision [text] started from. */
    val base: String,
    /** Bumped on every edit; answers are applied only to the revision they describe. */
    val revision: Long = 0,
    /** Token spans from the PC's lexer, carried across edits; null before the first answer. */
    val tokens: List<ByteSpan>? = null,
    /** Diagnostic spans from the PC's last analysis, carried across edits. */
    val marks: List<ByteSpan> = emptyList(),
    val conflict: Conflict? = null,
    /** The file was deleted on the PC while open here. */
    val deletedOnPc: Boolean = false,
) {
    val dirty: Boolean get() = text != saved
    val name: String get() = id.substringAfterLast('/')

    /** The person typed: the text changes at once, and every span moves with it. */
    fun edited(newText: String): OpenDocument {
        if (newText == text) return this
        val edit = editBetween(Utf8Index(text), Utf8Index(newText))
        return copy(
            text = newText,
            revision = revision + 1,
            tokens = tokens?.let { carry(it, edit) },
            marks = carry(marks, edit),
        )
    }

    fun withTokens(spans: List<ByteSpan>, atRevision: Long): OpenDocument =
        if (atRevision == revision) copy(tokens = spans) else this

    fun withMarks(spans: List<ByteSpan>, atRevision: Long): OpenDocument =
        if (atRevision == revision) copy(marks = spans) else this

    /**
     * The PC acknowledged a save of `submitted`. Only acknowledged bytes become
     * the saved baseline; anything typed while the save was in flight stays an
     * unsaved edit on top of it.
     */
    fun savedAs(submitted: String, digest: String, finalLineFeedAdded: Boolean, atRevision: Long): OpenDocument {
        val persisted = if (finalLineFeedAdded) submitted + "\n" else submitted
        return if (revision == atRevision && text == submitted) {
            copy(text = persisted, saved = persisted, base = digest, conflict = null, deletedOnPc = false)
        } else {
            copy(saved = persisted, base = digest, conflict = null, deletedOnPc = false)
        }
    }

    /** What to do about a change to this file on the PC. */
    fun remoteChange(digest: String?): RemoteChange = when {
        digest == base -> RemoteChange.SAME
        digest == null -> RemoteChange.DELETED
        !dirty -> RemoteChange.REFRESH
        else -> RemoteChange.CONFLICT
    }

    /** Take the PC's text, discarding nothing that was not already discarded. */
    fun reloaded(pcText: String, digest: String): OpenDocument = copy(
        text = pcText,
        saved = pcText,
        base = digest,
        revision = revision + 1,
        tokens = null,
        marks = emptyList(),
        conflict = null,
        deletedOnPc = false,
    )

    fun inConflict(pcText: String?, pcDigest: String): OpenDocument = copy(conflict = Conflict(pcText, pcDigest))

    /**
     * The person chose to keep their version over the PC's newer one. Their
     * text now counts as an edit of the PC's revision, so the next save is an
     * ordinary save of it — decided by them, not by whoever wrote last.
     */
    fun keepMine(): OpenDocument {
        val conflict = conflict ?: return this
        return copy(base = conflict.digest, saved = conflict.text ?: saved, conflict = null)
    }
}

data class Conflict(val text: String?, val digest: String)

enum class RemoteChange { SAME, REFRESH, CONFLICT, DELETED }

/** LCL document names, as the PC applies them: see `lcl_project::naming`. */
object LclNames {
    const val SUFFIX = ".lcl"
    const val TEXT_SUFFIX = ".lcl.txt"

    /** Only the exact `.lcl` and `.lcl.txt` endings; an ordinary `.txt` file is not LCL. */
    fun isDocument(name: String): Boolean =
        listOf(TEXT_SUFFIX, SUFFIX).any { name.length > it.length && name.endsWith(it) }

    /** The name a new document gets: an explicit ending is kept, never converted. */
    fun defaultName(name: String, ending: String = SUFFIX): String {
        val trimmed = name.trim()
        if (trimmed.endsWith(TEXT_SUFFIX) || trimmed.endsWith(SUFFIX)) return trimmed
        return trimmed + if (ending == TEXT_SUFFIX) TEXT_SUFFIX else SUFFIX
    }
}
