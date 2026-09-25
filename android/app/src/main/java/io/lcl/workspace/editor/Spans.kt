package io.lcl.workspace.editor

/**
 * Byte offsets and character offsets, and spans that move with an edit.
 *
 * The PC's engine speaks in UTF-8 byte offsets — `02_LEXICAL/01` makes source
 * bytes normative — and Compose in UTF-16 character offsets. Every conversion
 * goes through one [Utf8Index] per text, and nothing here counts characters to
 * find a line or decides anything about LCL: spans come from the engine and
 * are only moved or dropped, never made up.
 */
class Utf8Index(val text: String) {
    /** `byteAt[i]` is the UTF-8 offset of UTF-16 index `i`; one past the end too. */
    private val byteAt = IntArray(text.length + 1)
    val byteLength: Int

    init {
        var bytes = 0
        var i = 0
        while (i < text.length) {
            byteAt[i] = bytes
            val cp = text.codePointAt(i)
            val size = when {
                cp < 0x80 -> 1
                cp < 0x800 -> 2
                cp < 0x10000 -> 3
                else -> 4
            }
            if (Character.charCount(cp) == 2) byteAt[i + 1] = bytes
            bytes += size
            i += Character.charCount(cp)
        }
        byteAt[text.length] = bytes
        byteLength = bytes
    }

    fun byteOf(char: Int): Int = byteAt[char.coerceIn(0, text.length)]

    /** The character index at a byte offset: the start of the character holding it. */
    fun charOf(byte: Int): Int {
        if (byte <= 0) return 0
        if (byte >= byteLength) return text.length
        var lo = 0
        var hi = text.length
        while (lo < hi) {
            val mid = (lo + hi + 1) ushr 1
            if (byteAt[mid] <= byte) lo = mid else hi = mid - 1
        }
        // Step back from the second unit of a surrogate pair to its start.
        return if (lo > 0 && Character.isLowSurrogate(text[lo]) && Character.isHighSurrogate(text[lo - 1])) lo - 1 else lo
    }
}

/** A span of UTF-8 bytes the engine described. `line` is set for diagnostics. */
data class ByteSpan(val start: Int, val end: Int, val kind: String, val line: Int? = null)

/** One edit: bytes [from, oldTo) of the old text became [from, newTo) of the new one. */
data class Edit(val from: Int, val oldTo: Int, val newTo: Int, val lines: Int)

/** The smallest range that changed between two texts. */
fun editBetween(before: Utf8Index, after: Utf8Index): Edit {
    val a = before.text
    val b = after.text
    val shorter = minOf(a.length, b.length)
    var head = 0
    while (head < shorter && a[head] == b[head]) head++
    var tail = 0
    while (tail < shorter - head && a[a.length - 1 - tail] == b[b.length - 1 - tail]) tail++
    // A tail that starts on the second unit of a changed pair gives it back.
    if (tail > 0 && Character.isLowSurrogate(b[b.length - tail])) tail--
    fun feeds(text: String, from: Int, to: Int) = (from until to).count { text[it] == '\n' }
    return Edit(
        from = before.byteOf(head),
        oldTo = before.byteOf(a.length - tail),
        newTo = after.byteOf(b.length - tail),
        lines = feeds(b, head, b.length - tail) - feeds(a, head, a.length - tail),
    )
}

/**
 * Carry spans across one edit. A span wholly ahead of it stays; one wholly
 * after it moves with its text; one the edit touched is dropped, and what it
 * covered is drawn plain until the engine describes the new text.
 */
fun carry(spans: List<ByteSpan>, edit: Edit): List<ByteSpan> {
    val shift = edit.newTo - edit.oldTo
    return spans.mapNotNull { span ->
        when {
            span.start < edit.from && span.end <= edit.from -> span
            span.start >= edit.oldTo -> span.copy(
                start = span.start + shift,
                end = span.end + shift,
                line = span.line?.plus(edit.lines),
            )
            else -> null
        }
    }
}

/** Undo and redo over whole-text snapshots, with typing runs merged into one step. */
class EditHistory(private val limit: Int = 200, private val mergeWithinMs: Long = 800) {
    private val undo = ArrayDeque<Snapshot>()
    private val redo = ArrayDeque<Snapshot>()
    private var lastAt = 0L

    data class Snapshot(val text: String, val selectionStart: Int, val selectionEnd: Int)

    val canUndo: Boolean get() = undo.isNotEmpty()
    val canRedo: Boolean get() = redo.isNotEmpty()

    /** Record the state *before* an edit made at `atMs`. */
    fun record(before: Snapshot, atMs: Long) {
        if (undo.isEmpty() || atMs - lastAt > mergeWithinMs) {
            undo.addLast(before)
            if (undo.size > limit) undo.removeFirst()
        }
        lastAt = atMs
        redo.clear()
    }

    fun undo(current: Snapshot): Snapshot? {
        val previous = undo.removeLastOrNull() ?: return null
        redo.addLast(current)
        lastAt = 0
        return previous
    }

    fun redo(current: Snapshot): Snapshot? {
        val next = redo.removeLastOrNull() ?: return null
        undo.addLast(current)
        lastAt = 0
        return next
    }

    fun clear() {
        undo.clear()
        redo.clear()
        lastAt = 0
    }
}

/** Lines in a text, as the gutter counts them: by line feeds, with line 1 always there. */
fun lineCount(text: String): Int = text.count { it == '\n' } + 1
