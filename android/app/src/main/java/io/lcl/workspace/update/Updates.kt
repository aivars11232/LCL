package io.lcl.workspace.update

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull
import java.security.KeyFactory
import java.security.MessageDigest
import java.security.Signature
import java.security.spec.X509EncodedKeySpec

/**
 * Update System V1, the phone's side: the same contracts the PC updater keeps.
 *
 * One source (the latest stable GitHub Release of aivars11232/LCL), one trust
 * root (the update signing keys built into the app, from
 * update/trusted_keys.txt) and one signed statement of what a release holds
 * (update-manifest.json). The signature is checked before anything in the
 * manifest is read, and the APK is checked against the signed manifest and
 * against the installed app before Android is asked to install it.
 */
class UpdateRefused(val kind: String, message: String) : Exception(message)

/** An LCL product version, compared by Semantic Versioning 2.0.0 precedence. */
class SemVer private constructor(
    val major: Long,
    val minor: Long,
    val patch: Long,
    private val pre: List<String>,
) : Comparable<SemVer> {
    val isPrerelease get() = pre.isNotEmpty()

    override fun compareTo(other: SemVer): Int {
        compareValuesBy(this, other, { it.major }, { it.minor }, { it.patch }).let { if (it != 0) return it }
        if (pre.isEmpty() || other.pre.isEmpty()) return other.pre.size.coerceAtMost(1) - pre.size.coerceAtMost(1)
        for ((a, b) in pre.zip(other.pre)) {
            val (na, nb) = a.toLongOrNull() to b.toLongOrNull()
            val order = when {
                na != null && nb != null -> na.compareTo(nb)
                na != null -> -1
                nb != null -> 1
                else -> a.compareTo(b)
            }
            if (order != 0) return order
        }
        return pre.size.compareTo(other.pre.size)
    }

    override fun equals(other: Any?) = other is SemVer && compareTo(other) == 0
    override fun hashCode() = listOf(major, minor, patch, pre).hashCode()
    override fun toString() = "$major.$minor.$patch" + if (pre.isEmpty()) "" else "-" + pre.joinToString(".")

    companion object {
        private val number = Regex("0|[1-9][0-9]{0,17}")
        private val identifier = Regex("[0-9A-Za-z-]+")

        fun parse(text: String): SemVer {
            val bad = UpdateRefused("invalid", "\"$text\" is not a product version")
            val core = text.substringBefore('-')
            val pre = if ('-' in text) text.substringAfter('-').split('.') else emptyList()
            val parts = core.split('.')
            if (parts.size != 3 || parts.any { !number.matches(it) }) throw bad
            for (id in pre) {
                if (!identifier.matches(id)) throw bad
                if (id.all(Char::isDigit) && id.length > 1 && id.startsWith('0')) throw bad
            }
            return SemVer(parts[0].toLong(), parts[1].toLong(), parts[2].toLong(), pre)
        }
    }
}

data class AndroidArtifact(
    val artifactName: String,
    val size: Long,
    val sha256: String,
    val applicationId: String,
    val versionName: String,
    val versionCode: Long,
    val minimumSdk: Int,
    val signerSha256: String,
)

data class UpdateManifest(
    val productVersion: SemVer,
    val releaseTag: String,
    val publishedAt: String,
    val releaseNotes: String,
    val minimumSupportedVersion: SemVer,
    val signingKeyId: String,
    val pcArtifactName: String,
    val android: AndroidArtifact,
)

const val APPLICATION_ID = "io.lcl.workspace"
const val MAX_APK = 256L * 1024 * 1024
const val MAX_MANIFEST = 64L * 1024

private val hex64 = Regex("[0-9a-f]{64}")

fun sha256(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

/** A manifest whose signature already verified: every key required, nothing else allowed. */
fun parseManifest(bytes: ByteArray): UpdateManifest {
    fun refuse(why: String): Nothing = throw UpdateRefused("verification", "the update manifest: $why")
    if (bytes.size > MAX_MANIFEST) refuse("too large")
    val json = runCatching { Json.parseToJsonElement(bytes.toString(Charsets.UTF_8)) as JsonObject }.getOrNull() ?: refuse("not a JSON object")
    fun exactly(obj: JsonObject, what: String, keys: Set<String>) {
        if (obj.keys != keys) refuse("$what must have exactly ${keys.sorted()}")
    }
    fun JsonObject.text(key: String): String = (get(key) as? JsonPrimitive)?.takeIf { it.isString }?.content ?: refuse("$key must be a string")
    fun JsonObject.whole(key: String): Long = (get(key) as? JsonPrimitive)?.takeIf { !it.isString }?.longOrNull?.takeIf { it >= 0 } ?: refuse("$key must be a whole number")
    fun JsonObject.obj(key: String): JsonObject = get(key) as? JsonObject ?: refuse("$key must be an object")

    exactly(json, "the manifest", setOf("format", "product", "channel", "product_version", "release_tag", "source_commit",
        "published_at", "release_notes", "minimum_supported_version", "signing_key_id", "pc", "android"))
    if (json.whole("format") != 1L) refuse("format ${json["format"]} is not 1")
    if (json.text("product") != "lcl") refuse("not for LCL")
    if (json.text("channel") != "stable") refuse("not for the stable channel")
    val version = SemVer.parse(json.text("product_version"))
    if (version.isPrerelease) refuse("a pre-release is never offered on the stable channel")
    val tag = json.text("release_tag")
    if (tag != "v$version") refuse("release tag $tag does not name $version")
    if (!Regex("[0-9a-f]{40}").matches(json.text("source_commit"))) refuse("source_commit")
    val published = json.text("published_at")
    if (!Regex("[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z").matches(published)) refuse("published_at")
    val notes = json.text("release_notes")
    if (notes.length > 20_000) refuse("release notes too long")
    val minimum = SemVer.parse(json.text("minimum_supported_version"))
    if (minimum > version) refuse("minimum_supported_version is newer than the release")

    val pc = json.obj("pc")
    exactly(pc, "pc", setOf("artifact_name", "size", "sha256", "architecture", "required_updater_version"))
    if (pc.text("artifact_name") != "lcl-$version-linux-x86_64.tar.gz") refuse("pc.artifact_name")
    val a = json.obj("android")
    exactly(a, "android", setOf("artifact_name", "size", "sha256", "application_id", "version_name", "version_code", "minimum_sdk", "signer_sha256"))
    val artifact = AndroidArtifact(
        artifactName = a.text("artifact_name"),
        size = a.whole("size"),
        sha256 = a.text("sha256"),
        applicationId = a.text("application_id"),
        versionName = a.text("version_name"),
        versionCode = a.whole("version_code"),
        minimumSdk = a.whole("minimum_sdk").toInt(),
        signerSha256 = a.text("signer_sha256"),
    )
    if (artifact.size !in 1..MAX_APK) refuse("android.size")
    if (!hex64.matches(artifact.sha256) || !hex64.matches(artifact.signerSha256)) refuse("android digests")
    if (artifact.applicationId != APPLICATION_ID) refuse("android.application_id")
    if (artifact.versionName != version.toString()) refuse("android.version_name")
    if (artifact.versionCode !in 1..Int.MAX_VALUE.toLong()) refuse("android.version_code")
    if (artifact.artifactName != "lcl-android-${artifact.versionName}-${artifact.versionCode}.apk") refuse("android.artifact_name")
    return UpdateManifest(version, tag, published, notes, minimum, json.text("signing_key_id"), pc.text("artifact_name"), artifact)
}

/** One trusted ECDSA P-256 update signing key. */
class TrustedKey(val id: String, spki: ByteArray) {
    val key = runCatching { KeyFactory.getInstance("EC").generatePublic(X509EncodedKeySpec(spki)) }
        .getOrElse { throw UpdateRefused("not_configured", "trusted key $id is not an EC public key") }

    companion object {
        private val idPattern = Regex("[a-z0-9-]{1,64}")

        /** `<id> <hex SPKI DER>` per line, as update/trusted_keys.txt lists them. */
        fun parse(text: String): List<TrustedKey> = text.lines().map { it.trim() }
            .filter { it.isNotEmpty() && !it.startsWith("#") }
            .map { line ->
                val fields = line.split(Regex("\\s+"))
                if (fields.size != 2 || !idPattern.matches(fields[0]) || !Regex("([0-9a-f]{2})+").matches(fields[1])) {
                    throw UpdateRefused("not_configured", "a trusted key line is malformed")
                }
                TrustedKey(fields[0], fields[1].chunked(2).map { it.toInt(16).toByte() }.toByteArray())
            }
            .also { keys -> if (keys.map { it.id }.toSet().size != keys.size) throw UpdateRefused("not_configured", "a trusted key is listed twice") }
    }
}

const val NOT_CONFIGURED = "this build of LCL trusts no update signing key yet, so it cannot verify, and never installs, any update"

/** The manifest, read only once a trusted key's signature over its exact bytes verified. */
fun verifyManifest(bytes: ByteArray, signature: ByteArray, keys: List<TrustedKey>): UpdateManifest {
    if (keys.isEmpty()) throw UpdateRefused("not_configured", NOT_CONFIGURED)
    val signer = keys.firstOrNull { key ->
        runCatching {
            Signature.getInstance("SHA256withECDSA").run { initVerify(key.key); update(bytes); verify(signature) }
        }.getOrDefault(false)
    } ?: throw UpdateRefused("verification", "the update manifest's signature is not valid for any key this build trusts")
    val manifest = parseManifest(bytes)
    if (manifest.signingKeyId != signer.id) {
        throw UpdateRefused("verification", "the manifest names key ${manifest.signingKeyId}, but key ${signer.id} signed it")
    }
    return manifest
}

/** What is installed now. */
data class InstalledApp(val versionName: String, val versionCode: Long, val sdk: Int, val signers: Set<String>)

/** What an APK file says it is, as Android reads it. */
data class ApkFacts(val packageName: String, val versionName: String, val versionCode: Long, val signers: Set<String>)

/** Whether the manifest's release is newer than the installed app, and installable here. */
fun isNewer(manifest: UpdateManifest, installed: InstalledApp): Boolean {
    val current = runCatching { SemVer.parse(installed.versionName) }.getOrNull()
    if (current != null && manifest.productVersion <= current) return false
    if (manifest.android.versionCode <= installed.versionCode) {
        if (current == null) return false
        throw UpdateRefused("invalid", "LCL ${manifest.productVersion} does not advance the app's versionCode")
    }
    if (current != null && current < manifest.minimumSupportedVersion) {
        throw UpdateRefused("unsupported", "LCL ${manifest.productVersion} replaces ${manifest.minimumSupportedVersion} or newer only; install it manually")
    }
    if (installed.sdk < manifest.android.minimumSdk) {
        throw UpdateRefused("unsupported", "LCL ${manifest.productVersion} needs Android API ${manifest.android.minimumSdk}")
    }
    return true
}

/** The downloaded APK is exactly the signed one, is this app, is newer and is signed like the installed app. */
fun checkApk(bytes: ByteArray, facts: ApkFacts?, manifest: UpdateManifest, installed: InstalledApp) {
    val a = manifest.android
    if (bytes.size.toLong() != a.size || sha256(bytes) != a.sha256) {
        throw UpdateRefused("verification", "the downloaded APK does not match the size and SHA-256 its manifest signed")
    }
    facts ?: throw UpdateRefused("verification", "the downloaded file is not an APK Android can read")
    if (facts.packageName != APPLICATION_ID) throw UpdateRefused("verification", "the APK is ${facts.packageName}, not LCL")
    if (facts.versionCode != a.versionCode || facts.versionName != a.versionName) {
        throw UpdateRefused("verification", "the APK is ${facts.versionName} (${facts.versionCode}), not the signed ${a.versionName} (${a.versionCode})")
    }
    if (facts.versionCode <= installed.versionCode) throw UpdateRefused("invalid", "the APK is not newer than the app installed")
    if (facts.signers.isEmpty() || facts.signers != installed.signers || a.signerSha256 !in facts.signers) {
        throw UpdateRefused("verification", "the APK is not signed by the same key as the LCL installed, so Android could not update it in place")
    }
}

/** A release, as the latest-release listing names it. */
data class Release(val tag: String, val assets: Map<String, Long>)

/** The listing's release, if it is published and stable. */
fun parseRelease(text: String): Release? {
    val json = runCatching { Json.parseToJsonElement(text) as JsonObject }.getOrNull()
        ?: throw UpdateRefused("invalid", "the release listing is not JSON")
    fun flag(key: String) = (json[key] as? JsonPrimitive)?.booleanOrNull ?: throw UpdateRefused("invalid", "the release listing has no $key")
    if (flag("draft") || flag("prerelease")) return null
    val tag = (json["tag_name"] as? JsonPrimitive)?.contentOrNull?.takeIf { Regex("[A-Za-z0-9._-]{1,200}").matches(it) }
        ?: throw UpdateRefused("invalid", "the release listing has no usable tag")
    val assets = (json["assets"] as? kotlinx.serialization.json.JsonArray ?: throw UpdateRefused("invalid", "the release listing has no assets"))
        .mapNotNull { element: JsonElement ->
            val obj = element as? JsonObject ?: return@mapNotNull null
            val name = (obj["name"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
            val size = (obj["size"] as? JsonPrimitive)?.longOrNull ?: return@mapNotNull null
            name to size
        }.toMap()
    return Release(tag, assets)
}
