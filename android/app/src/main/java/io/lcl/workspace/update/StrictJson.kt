package io.lcl.workspace.update

/**
 * A strict JSON reader (RFC 8259) with exactly the rules of the PC updater's
 * (impl/crates/lcl-spec/src/json.rs), so the phone and the PC read one update
 * manifest the same way: no duplicate keys, no leading zeros or `+`, no
 * unquoted words, no lone surrogates, no raw control characters in strings, no
 * trailing content, nesting at most [MAX_NESTING] deep. Numbers are read by
 * value, as doubles. Objects keep their order.
 *
 * A value is a [Map] (String keys), a [List], a [String], a [Double], a
 * [Boolean] or [Null].
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
        if (reader.i != text.length) reader.fail("trailing content after top-level value")
        return value
    }

    private class Reader(private val s: String) {
        var i = 0
        private var depth = 0

        fun fail(why: String): Nothing = throw Malformed("$why at $i")

        private fun peek(): Char? = if (i < s.length) s[i] else null

        private fun digit() = peek()?.let { it in '0'..'9' } == true

        fun whitespace() {
            while (peek().let { it == ' ' || it == '\t' || it == '\n' || it == '\r' }) i++
        }

        private fun expect(c: Char) {
            if (peek() != c) fail("expected '$c'")
            i++
        }

        fun value(): Any = when (peek()) {
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
            if (!s.startsWith(word, i)) fail("invalid literal, expected '$word'")
            i += word.length
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
                i++
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
                    ',' -> i++
                    '}' -> {
                        i++
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
                i++
                return items
            }
            while (true) {
                whitespace()
                items += value()
                whitespace()
                when (peek()) {
                    ',' -> i++
                    ']' -> {
                        i++
                        return items
                    }
                    else -> fail("expected ',' or ']' in array")
                }
            }
        }

        private fun hex4(): Int {
            if (i + 4 > s.length) fail("truncated \\u escape")
            var v = 0
            repeat(4) {
                val c = s[i]
                val d = when (c) {
                    in '0'..'9' -> c - '0'
                    in 'a'..'f' -> c - 'a' + 10
                    in 'A'..'F' -> c - 'A' + 10
                    else -> fail("invalid hex digit in \\u escape")
                }
                v = (v shl 4) or d
                i++
            }
            return v
        }

        fun string(): String {
            expect('"')
            val out = StringBuilder()
            while (true) {
                val c = peek() ?: fail("unterminated string")
                when {
                    c == '"' -> {
                        i++
                        return out.toString()
                    }
                    c == '\\' -> {
                        i++
                        val e = peek() ?: fail("unterminated escape")
                        i++
                        when (e) {
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
                                        i++
                                        if (peek() != 'u') fail("unpaired high surrogate")
                                        i++
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
                        i++
                    }
                }
            }
        }

        private fun number(): Double {
            val start = i
            if (peek() == '-') i++
            when {
                peek() == '0' -> i++
                digit() -> while (digit()) i++
                else -> fail("invalid number")
            }
            if (peek() == '.') {
                i++
                if (!digit()) fail("expected digit after decimal point")
                while (digit()) i++
            }
            if (peek() == 'e' || peek() == 'E') {
                i++
                if (peek() == '+' || peek() == '-') i++
                if (!digit()) fail("expected digit in exponent")
                while (digit()) i++
            }
            return s.substring(start, i).toDouble()
        }
    }
}
