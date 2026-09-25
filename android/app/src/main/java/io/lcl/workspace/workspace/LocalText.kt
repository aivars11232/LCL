package io.lcl.workspace.workspace

import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.nio.ByteBuffer
import java.nio.CharBuffer
import java.nio.charset.CodingErrorAction

/**
 * A `.lcl` or `.lcl.txt` file from elsewhere on this phone, read as exactly
 * the text its bytes are — or refused.
 *
 * LCL source is UTF-8, and nothing may repair it before it is judged
 * (`02_LEXICAL/01`). A lenient decoder turns each malformed byte into U+FFFD,
 * and the PC would then check text the file does not contain. So a file that
 * is not valid UTF-8 is not opened at all, and nothing of it reaches the PC.
 */
object LocalText {
    /** The largest file opened. */
    const val LIMIT = 4 * 1024 * 1024

    class Refused(message: String) : Exception(message)

    /** At most [LIMIT] bytes from [stream], decoded strictly. */
    fun read(stream: InputStream): String {
        val bytes = ByteArrayOutputStream()
        val buffer = ByteArray(64 * 1024)
        while (true) {
            val read = stream.read(buffer)
            if (read < 0) break
            bytes.write(buffer, 0, read)
            if (bytes.size() > LIMIT) throw Refused("The file is larger than 4 MB.")
        }
        return decode(bytes.toByteArray())
    }

    /** [bytes] as text, exactly, or [Refused] naming the first byte that is not UTF-8. */
    fun decode(bytes: ByteArray): String {
        val decoder = Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
        val input = ByteBuffer.wrap(bytes)
        // UTF-8 never decodes to more UTF-16 units than it has bytes.
        val output = CharBuffer.allocate(bytes.size)
        var result = decoder.decode(input, output, true)
        if (!result.isError) result = decoder.flush(output)
        if (result.isError || result.isOverflow) {
            throw Refused(
                "This file is not valid UTF-8 (byte ${input.position()} is not), so it was not opened. " +
                    "An LCL document is UTF-8, and this app does not repair one.",
            )
        }
        return String(output.array(), 0, output.position())
    }
}
