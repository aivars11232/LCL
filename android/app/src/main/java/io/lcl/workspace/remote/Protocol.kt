package io.lcl.workspace.remote

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.security.MessageDigest

/**
 * The remote protocol this app speaks, `lcl.remote/1`: JSON messages in
 * length-prefixed frames, inside TLS 1.3. Not the LCL language version and not
 * the engine protocol; those come from the PC.
 */
object Protocol {
    const val VERSION = "lcl.remote/1"

    fun helloPair(code: String, deviceName: String): JsonObject = buildJsonObject {
        put("type", "hello")
        put("protocol", VERSION)
        put("intent", "pair")
        put("code", code)
        put("name", deviceName)
    }

    fun helloConnect(deviceId: String?): JsonObject = buildJsonObject {
        put("type", "hello")
        put("protocol", VERSION)
        put("intent", "connect")
        if (deviceId != null) put("device", deviceId)
    }

    fun request(id: Long, op: String, fields: JsonObject): JsonObject = JsonObject(
        buildMap {
            put("type", JsonPrimitive("request"))
            put("id", JsonPrimitive(id))
            put("op", JsonPrimitive(op))
            putAll(fields)
        },
    )

    fun parse(text: String): JsonObject =
        (Json.parseToJsonElement(text) as? JsonObject) ?: throw IOException("the PC sent something other than a JSON object")
}

/** One frame: a four-byte big-endian length, then that many bytes of JSON. */
object Frames {
    const val MAX = 16 * 1024 * 1024

    fun write(out: DataOutputStream, message: String) {
        val bytes = message.toByteArray(Charsets.UTF_8)
        synchronized(out) {
            out.writeInt(bytes.size)
            out.write(bytes)
            out.flush()
        }
    }

    fun read(input: DataInputStream): String {
        val length = input.readInt()
        if (length < 0 || length > MAX) throw IOException("a frame of $length bytes is over the limit")
        val bytes = ByteArray(length)
        input.readFully(bytes)
        return Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
            .decode(ByteBuffer.wrap(bytes))
            .toString()
    }
}

/** One answer from the PC. `status` follows HTTP's meaning, as the routes do. */
data class Reply(val status: Int, val body: JsonElement) {
    val ok: Boolean get() = status in 200..299
    val obj: JsonObject get() = body as? JsonObject ?: JsonObject(emptyMap())
    val error: String? get() = obj.str("error")
}

class RemoteException(message: String, val status: Int = 0) : IOException(message)

fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
fun JsonObject.long(key: String): Long? = (this[key] as? JsonPrimitive)?.longOrNull
fun JsonObject.bool(key: String): Boolean? = (this[key] as? JsonPrimitive)?.booleanOrNull
fun JsonObject.obj(key: String): JsonObject? = this[key] as? JsonObject
fun JsonObject.arr(key: String): JsonArray? = this[key] as? JsonArray
val JsonElement.objOrNull: JsonObject? get() = this as? JsonObject
fun JsonElement?.isNullish(): Boolean = this == null || this is JsonNull

/** Lowercase hex SHA-256. */
fun sha256Hex(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

/** Equality that does not stop at the first difference. */
fun constantTimeEquals(a: String, b: String): Boolean =
    MessageDigest.isEqual(a.toByteArray(Charsets.UTF_8), b.toByteArray(Charsets.UTF_8))
