package io.lcl.workspace.manual

import java.security.MessageDigest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/**
 * The packaged Users Manual: every `*.md` file under the app's `manual` assets, copied at build
 * time from the repository's `users_manual/` together with its `MANIFEST.json` and the viewer the
 * desktop workspace serves. The manual is documentation only; nothing here reaches the PC or a
 * document.
 */
class ManualSnapshot(
    val version: String,
    /** The digest recorded in MANIFEST.json. */
    val manifestDigest: String,
    /** The digest of the files actually packaged, by [digest]. */
    val digest: String,
    val files: List<Pair<String, ByteArray>>,
) {
    /** The packaged files are exactly the snapshot the manifest names. */
    val matchesManifest: Boolean
        get() = digest == manifestDigest

    /** What the viewer reads: the same shape as the desktop's `/manual/snapshot`. */
    fun json(): String = buildJsonObject {
        put("version", version)
        put("digest", digest)
        put(
            "files",
            buildJsonArray {
                files.forEach { (name, bytes) ->
                    add(
                        buildJsonObject {
                            put("name", name)
                            put("text", bytes.toString(Charsets.UTF_8))
                        }
                    )
                }
            },
        )
    }
        .toString()

    companion object {
        /**
         * SHA-256 over every file in the given order: its name, a NUL, its length in decimal, a
         * NUL, then its bytes — the rule the desktop workspace and
         * `users_manual/tools/manual_manifest.py` use.
         */
        fun digest(files: List<Pair<String, ByteArray>>): String {
            val sha = MessageDigest.getInstance("SHA-256")
            for ((name, bytes) in files) {
                sha.update(name.toByteArray(Charsets.UTF_8))
                sha.update(0)
                sha.update(bytes.size.toString().toByteArray(Charsets.US_ASCII))
                sha.update(0)
                sha.update(bytes)
            }
            return sha.digest().joinToString("") { "%02x".format(it) }
        }

        /** Read the snapshot from a listing of the manual folder and a reader of its files. */
        fun load(names: List<String>, read: (String) -> ByteArray): ManualSnapshot {
            val manifest =
                Json.parseToJsonElement(read("MANIFEST.json").toString(Charsets.UTF_8))
                    as JsonObject
            val markdown =
                names
                    .filter { it.endsWith(".md") }
                    .sortedWith { a, b -> compareBytes(a.toByteArray(), b.toByteArray()) }
            val files = markdown.map { it to read(it) }
            return ManualSnapshot(
                version = manifest["version"]?.jsonPrimitive?.content ?: "",
                manifestDigest = manifest["digest"]?.jsonPrimitive?.content ?: "",
                digest = digest(files),
                files = files,
            )
        }

        private fun compareBytes(a: ByteArray, b: ByteArray): Int {
            for (i in 0 until minOf(a.size, b.size)) {
                val difference = (a[i].toInt() and 0xff) - (b[i].toInt() and 0xff)
                if (difference != 0) return difference
            }
            return a.size - b.size
        }
    }
}
