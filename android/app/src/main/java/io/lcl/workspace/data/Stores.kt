package io.lcl.workspace.data

import android.content.SharedPreferences
import androidx.core.content.edit
import io.lcl.workspace.remote.arr
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.str
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Plain key-value storage: SharedPreferences on a device, a map in tests. */
interface KeyValueStore {
    fun get(key: String): String?
    fun put(key: String, value: String?)
}

class PreferencesStore(private val preferences: SharedPreferences) : KeyValueStore {
    override fun get(key: String): String? = preferences.getString(key, null)

    /** Written before returning: a pairing record must be on disk before anything relies on it. */
    override fun put(key: String, value: String?) {
        preferences.edit(commit = true) { if (value == null) remove(key) else putString(key, value) }
    }
}

class MemoryStore : KeyValueStore {
    private val values = mutableMapOf<String, String>()
    override fun get(key: String): String? = values[key]
    override fun put(key: String, value: String?) {
        if (value == null) values.remove(key) else values[key] = value
    }
}

/**
 * A PC this device is paired with. Nothing here is secret: the private key
 * stays in Android Keystore under [keyAlias], and the PC is identified by the
 * fingerprint of its certificate, never by an address.
 */
data class PcRecord(
    val pcId: String,
    val name: String,
    val fingerprint: String,
    /** Where it has been reached: the pairing code's addresses, and later ones. */
    val addresses: List<String>,
    /** The id the PC gave this device when it paired. */
    val deviceId: String,
    val keyAlias: String,
    val pairedAt: Long,
    val lastConnected: Long? = null,
    val lastAddress: String? = null,
) {
    fun toJson(): JsonObject = buildJsonObject {
        put("pc_id", pcId)
        put("name", name)
        put("fingerprint", fingerprint)
        put("addresses", buildJsonArray { addresses.forEach { add(JsonPrimitive(it)) } })
        put("device_id", deviceId)
        put("key_alias", keyAlias)
        put("paired_at", pairedAt)
        put("last_connected", lastConnected?.let(::JsonPrimitive) ?: JsonNull)
        put("last_address", lastAddress?.let(::JsonPrimitive) ?: JsonNull)
    }

    companion object {
        fun fromJson(json: JsonObject): PcRecord? = runCatching {
            PcRecord(
                pcId = json.str("pc_id")!!,
                name = json.str("name") ?: "",
                fingerprint = json.str("fingerprint")!!,
                addresses = json.arr("addresses")?.map { it.jsonPrimitive.content } ?: emptyList(),
                deviceId = json.str("device_id")!!,
                keyAlias = json.str("key_alias")!!,
                pairedAt = json.long("paired_at") ?: 0,
                lastConnected = json.long("last_connected"),
                lastAddress = json.str("last_address"),
            )
        }.getOrNull()
    }
}

/** The PCs this device is paired with, and which one it works with now. */
class PcStore(private val store: KeyValueStore) {
    fun all(): List<PcRecord> {
        val text = store.get(KEY_PCS) ?: return emptyList()
        val array = runCatching { Json.parseToJsonElement(text) as JsonArray }.getOrNull() ?: return emptyList()
        return array.mapNotNull { (it as? JsonObject)?.let(PcRecord::fromJson) }
    }

    fun get(pcId: String): PcRecord? = all().firstOrNull { it.pcId == pcId }

    fun save(record: PcRecord) {
        val others = all().filterNot { it.pcId == record.pcId }
        write(others + record)
    }

    fun remove(pcId: String) {
        write(all().filterNot { it.pcId == pcId })
        if (activeId() == pcId) setActive(null)
    }

    fun activeId(): String? = store.get(KEY_ACTIVE)
    fun setActive(pcId: String?) = store.put(KEY_ACTIVE, pcId)

    private fun write(records: List<PcRecord>) =
        store.put(KEY_PCS, JsonArray(records.map { it.toJson() }).toString())

    private companion object {
        const val KEY_PCS = "pcs"
        const val KEY_ACTIVE = "active_pc"
    }
}

enum class Theme { SYSTEM, DARK, LIGHT }

/** Presentation only, and only on this device: nothing here reaches the PC. */
data class AppSettings(
    val theme: Theme = Theme.SYSTEM,
    val fontSize: Int = DEFAULT_FONT,
    val lineNumbers: Boolean = true,
) {
    companion object {
        const val MIN_FONT = 11
        const val MAX_FONT = 24
        const val DEFAULT_FONT = 14
    }
}

/** Settings, read defensively: anything unreadable falls back to its default. */
class SettingsStore(private val store: KeyValueStore) {
    fun load(): AppSettings {
        val text = store.get(KEY) ?: return AppSettings()
        val json = runCatching { Json.parseToJsonElement(text) as JsonObject }.getOrNull() ?: return AppSettings()
        if (json.long("version") != 1L) return AppSettings()
        val defaults = AppSettings()
        return AppSettings(
            theme = runCatching { Theme.valueOf(json.str("theme")!!) }.getOrDefault(defaults.theme),
            fontSize = json.long("font_size")?.toInt()
                ?.takeIf { it in AppSettings.MIN_FONT..AppSettings.MAX_FONT } ?: defaults.fontSize,
            lineNumbers = (json["line_numbers"] as? JsonPrimitive)?.content?.toBooleanStrictOrNull() ?: defaults.lineNumbers,
        )
    }

    fun save(settings: AppSettings) {
        store.put(
            KEY,
            buildJsonObject {
                put("version", 1)
                put("theme", settings.theme.name)
                put("font_size", settings.fontSize.coerceIn(AppSettings.MIN_FONT, AppSettings.MAX_FONT))
                put("line_numbers", settings.lineNumbers)
            }.toString(),
        )
    }

    private companion object {
        const val KEY = "settings"
    }
}
