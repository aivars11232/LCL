package io.lcl.workspace.workspace

import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.remote.Reply
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The fields of a request about one project: its id, then the [pairs] given. */
internal fun fields(project: String, vararg pairs: Pair<String, Any?>): JsonObject =
    buildJsonObject {
        put("project", project)
        for ((key, value) in pairs) {
            when (value) {
                is String -> put(key, value)
                is Long -> put(key, value)
                is Int -> put(key, value)
                is Boolean -> put(key, value)
                is JsonObject -> put(key, value)
                is JsonArray -> put(key, value)
            }
        }
    }

/**
 * One request to the PC, or null when the PC cannot be reached. Nothing is reported here: the
 * caller decides what to say.
 */
internal suspend fun ConnectionManager.requestOrNull(op: String, fields: JsonObject): Reply? =
    try {
        request(op, fields)
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        null
    }
