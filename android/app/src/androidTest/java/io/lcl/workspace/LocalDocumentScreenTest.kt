package io.lcl.workspace

import android.net.Uri
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.ui.LocalDocumentScreen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * A `.lcl` or `.lcl.txt` file opened from elsewhere on the phone is shown, and
 * can be sent to the PC, exactly as its bytes are — or, if they are not UTF-8,
 * not at all. The file is one this test writes into the app's own cache; the
 * screen reads it through the content resolver, as it reads any opened file.
 * No PC is needed.
 */
@RunWith(AndroidJUnit4::class)
class LocalDocumentScreenTest {
    @get:Rule
    val rule = createComposeRule()

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private val container get() = (context.applicationContext as LclApplication).container

    private fun exists(tag: String) = rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun textOf(tag: String): String =
        rule.onNodeWithTag(tag, useUnmergedTree = true).fetchSemanticsNode().config
            .getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text } ?: ""

    private fun source(): String =
        rule.onNodeWithTag("source").fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text ?: ""

    /** Open `bytes` as the file `name`, and wait until the screen has read it. */
    private fun open(name: String, bytes: ByteArray) {
        val file = File(context.cacheDir, name).apply { writeBytes(bytes) }
        rule.setContent { LocalDocumentScreen(container, Uri.fromFile(file), AppSettings(), onBack = {}) }
        rule.waitUntil("the file is read", 10_000) { exists("source") || exists("local_problem") }
    }

    @Test
    fun a_file_that_is_not_utf8_is_refused_and_never_shown_or_sent() {
        // "café" as a Windows-1252 editor saves it: 0xE9 is not UTF-8.
        val bytes = "LCL:\n    NAME: \"caf".toByteArray() + byteArrayOf(0xe9.toByte()) + "\"\n".toByteArray()
        open("latin1.lcl", bytes)
        assertFalse("a file that is not UTF-8 was shown, repaired: ${if (exists("source")) source() else ""}", exists("source"))
        assertFalse("text the file does not contain could be sent to the PC", exists("local_check") || exists("local_inspect"))
        assertTrue(textOf("local_problem"), textOf("local_problem").contains("not valid UTF-8"))
    }

    @Test
    fun a_utf8_file_is_shown_exactly_and_can_be_checked() {
        val text = "LCL:\n    NAME: \"café ā € 𝄞\"\n"
        open("good.lcl.txt", text.toByteArray())
        assertEquals(text, source())
        assertTrue(exists("local_check") && exists("local_inspect"))
        assertFalse(exists("local_problem"))
    }
}
