package io.lcl.workspace.update

/**
 * A strict JSON reader (RFC 8259) with exactly the rules of the PC updater's
 * (impl/crates/lcl-spec/src/json.rs), so the phone and the PC read one update manifest the same
 * way: no duplicate keys, no leading zeros or `+`, no unquoted words, no lone surrogates, no raw
 * control characters in strings, no trailing content, nesting at most [MAX_NESTING] deep. Numbers
 * are read by value, as doubles. Objects keep their order.
 *
 * A value is a [Map] (String keys), a [List], a [String], a [Double], a [Boolean] or [Null].
 */
internal object StrictJson {
    /** JSON's `null`. */
    object Null

    class Malformed(message: String) : Exception(message)

    const val MAX_NESTING = 128

    fun parse(text: String): Any {
        val reader = Reader(text)
        reader.whitespace()
        val value = reader.value()
        reader.whitespace()
        if (reader.at != text.length) reader.fail("trailing content after top-level value")
        return value
    }

    private class Reader(private val text: String) {
        var at = 0
        private var depth = 0

        fun fail(why: String): Nothing = throw Malformed("$why at $at")

        private fun peek(): Char? = if (at < text.length) text[at] else null

        private fun digit() = peek()?.let { it in '0'..'9' } == true

        fun whitespace() {
            while (peek().let { it == ' ' || it == '\t' || it == '\n' || it == '\r' }) at++
        }

        private fun expect(c: Char) {
            if (peek() != c) fail("expected '$c'")
            at++
        }

        fun value(): Any =
            when (peek()) {
                null -> fail("unexpected end of input")
                '{' -> nested { members() }
                '[' -> nested { items() }
                '"' -> string()
                't' -> literal("true", true)
                'f' -> literal("false", false)
                'n' -> literal("null", Null)
                else -> number()
            }

        private fun literal(word: String, value: Any): Any {
            if (!text.startsWith(word, at)) fail("invalid literal, expected '$word'")
            at += word.length
            return value
        }

        private fun <T> nested(read: () -> T): T {
            if (depth >= MAX_NESTING) fail("nesting deeper than $MAX_NESTING levels is refused")
            depth++
            try {
                return read()
            } finally {
                depth--
            }
        }

        private fun members(): Map<String, Any> {
            expect('{')
            val members = LinkedHashMap<String, Any>()
            whitespace()
            if (peek() == '}') {
                at++
                return members
            }
            while (true) {
                whitespace()
                val key = string()
                if (key in members) fail("duplicate object key '$key'")
                whitespace()
                expect(':')
                whitespace()
                members[key] = value()
                whitespace()
                when (peek()) {
                    ',' -> at++
                    '}' -> {
                        at++
                        return members
                    }
                    else -> fail("expected ',' or '}' in object")
                }
            }
        }

        private fun items(): List<Any> {
            expect('[')
            val items = mutableListOf<Any>()
            whitespace()
            if (peek() == ']') {
                at++
                return items
            }
            while (true) {
                whitespace()
                items += value()
                whitespace()
                when (peek()) {
                    ',' -> at++
                    ']' -> {
                        at++
                        return items
                    }
                    else -> fail("expected ',' or ']' in array")
                }
            }
        }

        private fun hex4(): Int {
            if (at + 4 > text.length) fail("truncated \\u escape")
            var value = 0
            repeat(4) {
                val c = text[at]
                val digit =
                    when (c) {
                        in '0'..'9' -> c - '0'
                        in 'a'..'f' -> c - 'a' + 10
                        in 'A'..'F' -> c - 'A' + 10
                        else -> fail("invalid hex digit in \\u escape")
                    }
                value = (value shl 4) or digit
                at++
            }
            return value
        }

        fun string(): String {
            expect('"')
            val out = StringBuilder()
            while (true) {
                val c = peek() ?: fail("unterminated string")
                when {
                    c == '"' -> {
                        at++
                        return out.toString()
                    }
                    c == '\\' -> {
                        at++
                        val escaped = peek() ?: fail("unterminated escape")
                        at++
                        when (escaped) {
                            '"' -> out.append('"')
                            '\\' -> out.append('\\')
                            '/' -> out.append('/')
                            'b' -> out.append('\b')
                            'f' -> out.append('\u000C')
                            'n' -> out.append('\n')
                            'r' -> out.append('\r')
                            't' -> out.append('\t')
                            'u' -> {
                                val hi = hex4()
                                when (hi) {
                                    in 0xD800..0xDBFF -> {
                                        if (peek() != '\\') fail("unpaired high surrogate")
                                        at++
                                        if (peek() != 'u') fail("unpaired high surrogate")
                                        at++
                                        val lo = hex4()
                                        if (lo !in 0xDC00..0xDFFF) fail("invalid low surrogate")
                                        out.append(hi.toChar()).append(lo.toChar())
                                    }
                                    in 0xDC00..0xDFFF -> fail("unexpected low surrogate")
                                    else -> out.append(hi.toChar())
                                }
                            }
                            else -> fail("invalid escape character")
                        }
                    }
                    c < ' ' -> fail("raw control character in string")
                    else -> {
                        // Text decoded strictly from UTF-8 holds only whole
                        // surrogate pairs, so each char is copied as it is.
                        out.append(c)
                        at++
                    }
                }
            }
        }

        private fun number(): Double {
            val start = at
            if (peek() == '-') at++
            when {
                peek() == '0' -> at++
                digit() -> while (digit()) at++
                else -> fail("invalid number")
            }
            if (peek() == '.') {
                at++
                if (!digit()) fail("expected digit after decimal point")
                while (digit()) at++
            }
            if (peek() == 'e' || peek() == 'E') {
                at++
                if (peek() == '+' || peek() == '-') at++
                if (!digit()) fail("expected digit in exponent")
                while (digit()) at++
            }
            return text.substring(start, at).toDouble()
        }
    }
}
