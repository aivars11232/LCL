package io.lcl.workspace.remote

import java.util.Base64

/**
 * What a pairing QR code says: the PC to trust, where to try reaching it, and
 * a one-time code.
 *
 * ```
 * lclpair://pair?v=1&pc=<id>&n=<name>&fp=<certificate SHA-256>
 *               &a=<host:port>[&a=...]&c=<one-time code>&e=<expiry>
 * ```
 *
 * `fp` is the PC's identity: the connection is refused unless the PC presents
 * exactly that certificate. Addresses are only where to look; they are never
 * part of who the PC is. The code is good for one pairing and a few minutes,
 * and is never stored. Mirrors `lcl-remote`'s `pairing::Link`, and refuses
 * the same malformed links.
 */
data class PairingLink(
    val version: Int,
    val pcId: String,
    val pcName: String,
    val fingerprint: String,
    val addresses: List<String>,
    val code: String,
    val expires: Long,
) {
    fun isExpired(nowSeconds: Long): Boolean = nowSeconds >= expires

    companion object {
        const val SCHEME = "lclpair"
        const val VERSION = 1

        /** Read a link, or throw [InvalidLink] saying what is wrong with it. */
        fun parse(uri: String): PairingLink {
            val query = uri.trim().removePrefix("$SCHEME://pair?").takeIf { it != uri.trim() }
                ?: throw InvalidLink("This is not an LCL pairing code.")
            val single = mutableMapOf<String, String>()
            val addresses = mutableListOf<String>()
            for (pair in query.split('&')) {
                val at = pair.indexOf('=')
                if (at < 0) throw InvalidLink("The pairing code is malformed.")
                val key = pair.substring(0, at)
                val value = percentDecode(pair.substring(at + 1))
                    ?: throw InvalidLink("The pairing code is badly encoded.")
                when (key) {
                    "a" -> addresses += value
                    "v", "pc", "n", "fp", "c", "e" ->
                        if (single.put(key, value) != null) throw InvalidLink("The pairing code names $key twice.")
                    else -> Unit // a later version's extra field
                }
            }
            val version = single["v"]?.toIntOrNull() ?: throw InvalidLink("The pairing code has no version.")
            if (version != VERSION) {
                throw InvalidLink("This pairing code is version $version; this app reads version $VERSION. Update the app or the PC.")
            }
            val fingerprint = single["fp"] ?: throw InvalidLink("The pairing code does not identify the PC.")
            if (fingerprint.length != 64 || fingerprint.any { it !in '0'..'9' && it !in 'a'..'f' }) {
                throw InvalidLink("The PC fingerprint in the pairing code is malformed.")
            }
            val code = single["c"] ?: throw InvalidLink("The pairing code has no one-time code.")
            val decoded = runCatching { Base64.getUrlDecoder().decode(code) }.getOrNull()
            if (decoded == null || decoded.size != 32 || code.contains('=')) {
                throw InvalidLink("The one-time code is malformed.")
            }
            if (addresses.isEmpty()) throw InvalidLink("The pairing code names no address for the PC.")
            return PairingLink(
                version = version,
                pcId = single["pc"] ?: throw InvalidLink("The pairing code has no PC id."),
                pcName = single["n"] ?: "",
                fingerprint = fingerprint,
                addresses = addresses,
                code = code,
                expires = single["e"]?.toLongOrNull() ?: throw InvalidLink("The pairing code has no expiry."),
            )
        }

        private fun percentDecode(text: String): String? {
            val out = java.io.ByteArrayOutputStream()
            var i = 0
            while (i < text.length) {
                val c = text[i]
                if (c == '%') {
                    // Two hex digits must follow.
                    if (i + 2 >= text.length) return null
                    val hex = text.substring(i + 1, i + 3)
                    if (!hex.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) return null
                    out.write(hex.toInt(16))
                    i += 3
                } else {
                    val bytes = c.toString().toByteArray(Charsets.UTF_8)
                    out.write(bytes, 0, bytes.size)
                    i += 1
                }
            }
            return runCatching {
                Charsets.UTF_8.newDecoder()
                    .onMalformedInput(java.nio.charset.CodingErrorAction.REPORT)
                    .decode(java.nio.ByteBuffer.wrap(out.toByteArray()))
                    .toString()
            }.getOrNull()
        }
    }
}

class InvalidLink(message: String) : Exception(message)
