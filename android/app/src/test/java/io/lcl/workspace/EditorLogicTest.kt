package io.lcl.workspace

import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.data.MemoryStore
import io.lcl.workspace.data.PcRecord
import io.lcl.workspace.data.PcStore
import io.lcl.workspace.data.SettingsStore
import io.lcl.workspace.data.Theme
import io.lcl.workspace.editor.ByteSpan
import io.lcl.workspace.editor.EditHistory
import io.lcl.workspace.editor.Utf8Index
import io.lcl.workspace.editor.carry
import io.lcl.workspace.editor.editBetween
import io.lcl.workspace.editor.lineCount
import io.lcl.workspace.workspace.LclNames
import io.lcl.workspace.workspace.OpenDocument
import io.lcl.workspace.workspace.RemoteChange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class Utf8IndexTest {
    @Test
    fun bytes_and_characters_map_both_ways() {
        val index = Utf8Index("aé€😀b") // 1 + 2 + 3 + 4 + 1 bytes
        assertEquals(11, index.byteLength)
        assertEquals(listOf(0, 1, 3, 6, 6, 10, 11), (0..6).map(index::byteOf))
        assertEquals(0, index.charOf(0))
        assertEquals(1, index.charOf(1))
        assertEquals(1, index.charOf(2)) // inside é
        assertEquals(3, index.charOf(6))
        assertEquals(3, index.charOf(8)) // inside the emoji: its first unit
        assertEquals(5, index.charOf(10))
        assertEquals(6, index.charOf(99))
    }
}

class CarryTest {
    private val text = "LCL:\n    VERSION: \"0.1.0\"\n"
    private val spans = listOf(ByteSpan(0, 3, "block"), ByteSpan(3, 4, "symbol"), ByteSpan(9, 16, "keyword", line = 2))

    private fun edit(after: String) = carry(spans, editBetween(Utf8Index(text), Utf8Index(after)))

    @Test
    fun typing_ahead_of_every_span_moves_them_all() {
        val moved = edit("XX$text")
        assertEquals(listOf(2 to 5, 5 to 6, 11 to 18), moved.map { it.start to it.end })
    }

    @Test
    fun typing_inside_a_span_drops_only_that_one() {
        val moved = edit(text.replace("VERSION", "VERXSION"))
        assertEquals(listOf("block", "symbol"), moved.map { it.kind })
    }

    @Test
    fun a_new_line_above_a_diagnostic_moves_its_line() {
        val moved = edit("\n\n$text")
        assertEquals(4, moved.last().line)
        val removed = carry(moved, editBetween(Utf8Index("\n\n$text"), Utf8Index(text)))
        assertEquals(2, removed.last().line)
    }

    @Test
    fun line_numbers_come_from_the_text() {
        assertEquals(1, lineCount(""))
        assertEquals(2, lineCount("LCL:\n"))
        assertEquals(4, lineCount("a\nb\nc\n"))
    }
}

class EditHistoryTest {
    @Test
    fun undo_and_redo_restore_text_and_cursor() {
        val history = EditHistory(mergeWithinMs = 100)
        history.record(EditHistory.Snapshot("a", 1, 1), atMs = 0)
        history.record(EditHistory.Snapshot("ab", 2, 2), atMs = 50) // merged: one typing run
        history.record(EditHistory.Snapshot("abc", 3, 3), atMs = 1_000)
        val back = history.undo(EditHistory.Snapshot("abcd", 4, 4))!!
        assertEquals("abc", back.text)
        assertEquals("a", history.undo(back)!!.text)
        assertFalse(history.canUndo)
        assertEquals("abc", history.redo(EditHistory.Snapshot("a", 1, 1))!!.text)
        assertTrue(history.canRedo)
    }
}

class OpenDocumentTest {
    private fun doc(text: String = "LCL:\n") = OpenDocument("p", "doc.lcl", text, text, "base0")

    @Test
    fun an_edit_is_on_screen_at_once_and_counts_as_unsaved() {
        val edited = doc().edited("LCL:\nX\n")
        assertEquals("LCL:\nX\n", edited.text)
        assertTrue(edited.dirty)
        assertEquals(1, edited.revision)
    }

    @Test
    fun answers_apply_only_to_the_revision_they_describe() {
        val first = doc()
        val edited = first.edited("LCL:\nX\n")
        val stale = edited.withTokens(listOf(ByteSpan(0, 3, "block")), atRevision = first.revision)
        assertNull("tokens for older text were applied", stale.tokens)
        val current = edited.withTokens(listOf(ByteSpan(0, 3, "block")), atRevision = edited.revision)
        assertEquals(1, current.tokens!!.size)
    }

    @Test
    fun a_save_acknowledged_after_more_typing_keeps_the_later_edits_unsaved() {
        val start = doc().edited("first")
        val submittedAt = start.revision
        val typedMore = start.edited("first and more")
        val acknowledged = typedMore.savedAs("first", "digest1", finalLineFeedAdded = true, atRevision = submittedAt)
        assertEquals("first and more", acknowledged.text)
        assertEquals("first\n", acknowledged.saved)
        assertEquals("digest1", acknowledged.base)
        assertTrue(acknowledged.dirty)
    }

    @Test
    fun a_change_on_the_pc_refreshes_a_clean_copy_and_stops_a_dirty_one() {
        val clean = doc()
        assertEquals(RemoteChange.SAME, clean.remoteChange("base0"))
        assertEquals(RemoteChange.REFRESH, clean.remoteChange("newer"))
        assertEquals(RemoteChange.DELETED, clean.remoteChange(null))
        val dirty = clean.edited("mine\n")
        assertEquals(RemoteChange.CONFLICT, dirty.remoteChange("newer"))
    }

    @Test
    fun keeping_mine_rebases_on_the_pc_revision_explicitly() {
        val dirty = doc().edited("mine\n").inConflict("theirs\n", "newer")
        assertEquals("base0", dirty.base)
        val kept = dirty.keepMine()
        assertEquals("newer", kept.base)
        assertEquals("mine\n", kept.text)
        assertEquals("theirs\n", kept.saved)
        assertTrue(kept.dirty)
        assertNull(kept.conflict)
        val reloaded = dirty.reloaded("theirs\n", "newer")
        assertEquals("theirs\n", reloaded.text)
        assertFalse(reloaded.dirty)
    }
}

class NamesAndStoresTest {
    @Test
    fun only_the_two_lcl_endings_are_documents_and_explicit_endings_are_kept() {
        assertTrue(LclNames.isDocument("a.lcl"))
        assertTrue(LclNames.isDocument("a.lcl.txt"))
        for (name in listOf("a.txt", ".lcl", ".lcl.txt", "a.LCL", "a.lcl.bak")) assertFalse(name, LclNames.isDocument(name))
        assertEquals("test.lcl", LclNames.defaultName("test"))
        assertEquals("test.lcl.txt", LclNames.defaultName("test", LclNames.TEXT_SUFFIX))
        assertEquals("test.lcl", LclNames.defaultName("test.lcl", LclNames.TEXT_SUFFIX))
        assertEquals("test.lcl.txt", LclNames.defaultName("test.lcl.txt"))
        assertEquals("notes.txt.lcl", LclNames.defaultName("notes.txt"))
    }

    @Test
    fun paired_pcs_survive_a_restart_of_the_store() {
        val backing = MemoryStore()
        val record = PcRecord("pc1", "Desk", "f".repeat(64), listOf("10.0.0.2:47300"), "dev1", "alias1", 100, 200, "10.0.0.2:47300")
        PcStore(backing).apply { save(record); setActive("pc1") }
        val reopened = PcStore(backing)
        assertEquals(record, reopened.get("pc1"))
        assertEquals("pc1", reopened.activeId())
        reopened.remove("pc1")
        assertNull(reopened.get("pc1"))
        assertNull(reopened.activeId())
    }

    @Test
    fun settings_are_bounded_and_fall_back_when_unreadable() {
        val backing = MemoryStore()
        val store = SettingsStore(backing)
        assertEquals(AppSettings(), store.load())
        store.save(AppSettings(Theme.DARK, 99, false))
        assertEquals(AppSettings(Theme.DARK, AppSettings.MAX_FONT, false), store.load())
        backing.put("settings", "{not json")
        assertEquals(AppSettings(), store.load())
        backing.put("settings", "{\"version\":1,\"theme\":\"NEON\",\"font_size\":3,\"line_numbers\":\"yes\"}")
        assertEquals(AppSettings(), store.load())
    }
}
