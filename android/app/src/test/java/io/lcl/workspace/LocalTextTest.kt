package io.lcl.workspace

import io.lcl.workspace.workspace.LocalText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayOutputStream

class LocalTextTest {
    @Test
    fun utf8_is_read_exactly() {
        val text = "LCL:\n    NAME: \"café ā € 𝄞\"\n"
        val bytes = text.toByteArray(Charsets.UTF_8)
        assertEquals(text, LocalText.decode(bytes))
        assertEquals(text, LocalText.read(bytes.inputStream()))
        assertTrue(LocalText.decode(bytes).toByteArray(Charsets.UTF_8).contentEquals(bytes))
        assertEquals("", LocalText.decode(ByteArray(0)))
    }

    @Test
    fun malformed_utf8_is_refused_and_never_repaired() {
        val before = "LCL:\n    NAME: \"caf".toByteArray()
        val after = "\"\n".toByteArray()
        for ((what, bad) in listOf(
            "Windows-1252 é" to bytes(0xe9),
            "a byte that starts nothing" to bytes(0xff),
            "a sequence cut short" to bytes(0xc3),
            "an overlong form" to bytes(0xc0, 0xaf),
            "an encoded surrogate" to bytes(0xed, 0xa0, 0x80),
            "past U+10FFFF" to bytes(0xf4, 0x90, 0x80, 0x80),
        )) {
            val file = before + bad + after
            // What a lenient decoder makes of it: text the file does not contain.
            val lenient = ByteArrayOutputStream().apply { write(file) }.toString(Charsets.UTF_8.name())
            assertTrue(what, lenient.contains(Char(0xFFFD)))
            val refused = assertThrows(what, LocalText.Refused::class.java) { LocalText.decode(file) }
            assertTrue("$what: ${refused.message}", refused.message!!.contains("not valid UTF-8 (byte ${before.size} is not)"))
            assertThrows(what, LocalText.Refused::class.java) { LocalText.read(file.inputStream()) }
        }
    }

    @Test
    fun the_four_mebibyte_limit_is_kept() {
        val largest = ByteArray(LocalText.LIMIT) { 'a'.code.toByte() }
        assertEquals(LocalText.LIMIT, LocalText.read(largest.inputStream()).length)
        val refused = assertThrows(LocalText.Refused::class.java) {
            LocalText.read((largest + 'a'.code.toByte()).inputStream())
        }
        assertEquals("The file is larger than 4 MB.", refused.message)
    }

    private fun bytes(vararg values: Int) = ByteArray(values.size) { values[it].toByte() }
}
