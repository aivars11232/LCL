package io.lcl.workspace.update

import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.security.KeyFactory
import java.security.MessageDigest
import java.security.Signature
import java.security.spec.X509EncodedKeySpec
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull

/**
 * Update System V1, the phone's side: the same contracts the PC updater keeps.
 *
 * One source (the latest stable GitHub Release of aivars11232/LCL), one trust root (the update
 * signing keys built into the app, from update/trusted_keys.txt) and one signed statement of what a
 * release holds (update-manifest.json). The signature is checked before anything in the manifest is
 * read, and the APK is checked against the signed manifest and against the installed app before
 * Android is asked to install it.
 */
class UpdateRefused(val kind: String, message: String) : Exception(message)

/**
 * An LCL product version, compared by Semantic Versioning 2.0.0 precedence, with the PC updater's
 * limits: every number, and every numeric pre-release identifier, fits an unsigned 64-bit integer;
 * build metadata is refused.
 */
class SemVer
private constructor(
    val major: ULong,
    val minor: ULong,
    val patch: ULong,
    /** Each identifier a [ULong] (numeric) or a [String] (alphanumeric). */
    private val pre: List<Any>,
) : Comparable<SemVer> {
    val isPrerelease
        get() = pre.isNotEmpty()

    override fun compareTo(other: SemVer): Int {
        compareValuesBy(this, other, { it.major }, { it.minor }, { it.patch }).let {
            if (it != 0) return it
        }
        if (pre.isEmpty() || other.pre.isEmpty())
            return other.pre.size.coerceAtMost(1) - pre.size.coerceAtMost(1)
        for ((a, b) in pre.zip(other.pre)) {
            val order =
                when {
                    a is ULong && b is ULong -> a.compareTo(b)
                    a is ULong -> -1
                    b is ULong -> 1
                    else -> (a as String).compareTo(b as String)
                }
            if (order != 0) return order
        }
        return pre.size.compareTo(other.pre.size)
    }

    override fun equals(other: Any?) = other is SemVer && compareTo(other) == 0

    override fun hashCode() = listOf(major, minor, patch, pre).hashCode()

    override fun toString() =
        "$major.$minor.$patch" + if (pre.isEmpty()) "" else "-" + pre.joinToString(".")

    companion object {
        private val digits = Regex("[0-9]+")
        private val identifier = Regex("[0-9A-Za-z-]+")

        fun parse(text: String): SemVer {
            val bad = UpdateRefused("invalid", "\"$text\" is not a product version")
            fun number(part: String): ULong {
                if (!digits.matches(part) || (part.length > 1 && part.startsWith('0'))) throw bad
                return part.toULongOrNull() ?: throw bad
            }
            val core = text.substringBefore('-').split('.')
            if (core.size != 3) throw bad
            val pre = if ('-' in text) text.substringAfter('-').split('.') else emptyList()
            val identifiers = pre.map { id ->
                if (!identifier.matches(id)) throw bad
                if (digits.matches(id)) number(id) else id
            }
            return SemVer(number(core[0]), number(core[1]), number(core[2]), identifiers)
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

data class PcArtifact(
    val artifactName: String,
    val size: Long,
    val sha256: String,
    val architecture: String,
    val requiredUpdaterVersion: Long,
)

data class UpdateManifest(
    val productVersion: SemVer,
    val releaseTag: String,
    val publishedAt: String,
    val releaseNotes: String,
    val minimumSupportedVersion: SemVer,
    val signingKeyId: String,
    val pc: PcArtifact,
    val android: AndroidArtifact,
)

const val APPLICATION_ID = "io.lcl.workspace"
const val MAX_APK = 256L * 1024 * 1024
const val MAX_PC_ARTIFACT = 512L * 1024 * 1024
const val MAX_MANIFEST = 64L * 1024

/** The largest value of a counted field: a version code, an API level, an updater protocol. */
private const val MAX_WHOLE = Int.MAX_VALUE.toLong()
private const val MAX_NOTES = 20_000

private val hex64 = Regex("[0-9a-f]{64}")
private val architecturePattern = Regex("[a-z0-9_-]{1,64}")

fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

fun sha256(bytes: ByteArray): String = hex(MessageDigest.getInstance("SHA-256").digest(bytes))

/**
 * A manifest whose signature already verified, read by the PC updater's rules
 * (update/src/manifest.rs): strict UTF-8 and strict JSON, every key required and nothing else
 * allowed, every value checked. Both read update/manifest_vectors/ in their tests and must agree on
 * every case. The PC alone later refuses a PC artifact for another architecture or a newer updater
 * protocol; neither concerns the phone.
 */
fun parseManifest(bytes: ByteArray): UpdateManifest {
    fun refuse(why: String): Nothing =
        throw UpdateRefused("verification", "the update manifest: $why")
    if (bytes.size > MAX_MANIFEST) refuse("too large")
    val text = runCatching {
        Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
            .decode(ByteBuffer.wrap(bytes))
            .toString()
    }
        .getOrElse { refuse("not UTF-8") }
    val json = runCatching {
        StrictJson.parse(text)
    }
        .getOrElse { refuse("not JSON: ${it.message}") }
    @Suppress("UNCHECKED_CAST")
    fun Any.members(what: String, keys: Set<String>): Map<String, Any> {
        val members = this as? Map<String, Any> ?: refuse("$what is not a JSON object")
        if (members.keys != keys) refuse("$what must have exactly ${keys.sorted()}")
        return members
    }
    fun Map<String, Any>.text(key: String): String =
        get(key) as? String ?: refuse("$key must be a string")
    // A whole number by value, as the PC reads one: 4 and 4.0 alike.
    fun Map<String, Any>.whole(key: String): Long =
        (get(key) as? Double)
            ?.takeIf { it.isFinite() && it >= 0.0 && it == Math.floor(it) }
            ?.toLong() ?: refuse("$key must be a whole number")
    fun Map<String, Any>.counted(key: String): Long =
        whole(key).takeIf { it in 1..MAX_WHOLE } ?: refuse("$key must be 1 to $MAX_WHOLE")
    fun Map<String, Any>.digest(key: String): String =
        text(key).takeIf { hex64.matches(it) }
            ?: refuse("$key must be 64 lowercase hexadecimal digits")

    val top =
        json.members(
            "the manifest",
            setOf(
                "format",
                "product",
                "channel",
                "product_version",
                "release_tag",
                "source_commit",
                "published_at",
                "release_notes",
                "minimum_supported_version",
                "signing_key_id",
                "pc",
                "android",
            ),
        )
    if (top.whole("format") != 1L) refuse("format ${top["format"]} is not 1")
    if (top.text("product") != "lcl") refuse("not for LCL")
    if (top.text("channel") != "stable") refuse("not for the stable channel")
    val version = SemVer.parse(top.text("product_version"))
    if (version.isPrerelease) refuse("a pre-release is never offered on the stable channel")
    val tag = top.text("release_tag")
    if (tag != "v$version") refuse("release tag $tag does not name $version")
    if (!Regex("[0-9a-f]{40}").matches(top.text("source_commit"))) refuse("source_commit")
    val published = top.text("published_at")
    if (!Regex("[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z").matches(published))
        refuse("published_at")
    val notes = top.text("release_notes")
    if (notes.codePointCount(0, notes.length) > MAX_NOTES) refuse("release notes too long")
    val minimum = SemVer.parse(top.text("minimum_supported_version"))
    if (minimum > version) refuse("minimum_supported_version is newer than the release")
    val keyId = top.text("signing_key_id")

    val p =
        top.getValue("pc")
            .members(
                "pc",
                setOf(
                    "artifact_name",
                    "size",
                    "sha256",
                    "architecture",
                    "required_updater_version",
                ),
            )
    val pc =
        PcArtifact(
            artifactName = p.text("artifact_name"),
            size = p.whole("size").takeIf { it in 1..MAX_PC_ARTIFACT } ?: refuse("pc.size"),
            sha256 = p.digest("sha256"),
            architecture =
                p.text("architecture").takeIf { architecturePattern.matches(it) }
                    ?: refuse("pc.architecture"),
            requiredUpdaterVersion = p.counted("required_updater_version"),
        )
    if (pc.artifactName != "lcl-$version-linux-x86_64.tar.gz") refuse("pc.artifact_name")

    val a =
        top.getValue("android")
            .members(
                "android",
                setOf(
                    "artifact_name",
                    "size",
                    "sha256",
                    "application_id",
                    "version_name",
                    "version_code",
                    "minimum_sdk",
                    "signer_sha256",
                ),
            )
    val artifact =
        AndroidArtifact(
            artifactName = a.text("artifact_name"),
            size = a.whole("size").takeIf { it in 1..MAX_APK } ?: refuse("android.size"),
            sha256 = a.digest("sha256"),
            applicationId = a.text("application_id"),
            versionName = a.text("version_name"),
            versionCode = a.counted("version_code"),
            minimumSdk = a.counted("minimum_sdk").toInt(),
            signerSha256 = a.digest("signer_sha256"),
        )
    if (artifact.applicationId != APPLICATION_ID) refuse("android.application_id")
    if (artifact.versionName != version.toString()) refuse("android.version_name")
    if (artifact.artifactName != "lcl-android-${artifact.versionName}-${artifact.versionCode}.apk")
        refuse("android.artifact_name")
    return UpdateManifest(version, tag, published, notes, minimum, keyId, pc, artifact)
}

/** One trusted ECDSA P-256 update signing key. */
class TrustedKey(val id: String, spki: ByteArray) {
    val key = runCatching {
        KeyFactory.getInstance("EC").generatePublic(X509EncodedKeySpec(spki))
    }
        .getOrElse {
            throw UpdateRefused("not_configured", "trusted key $id is not an EC public key")
        }

    companion object {
        private val idPattern = Regex("[a-z0-9-]{1,64}")

        /** `<id> <hex SPKI DER>` per line, as update/trusted_keys.txt lists them. */
        fun parse(text: String): List<TrustedKey> =
            text
                .lines()
                .map { it.trim() }
                .filter { it.isNotEmpty() && !it.startsWith("#") }
                .map { line ->
                    val fields = line.split(Regex("\\s+"))
                    if (
                        fields.size != 2 ||
                            !idPattern.matches(fields[0]) ||
                            !Regex("([0-9a-f]{2})+").matches(fields[1])
                    ) {
                        throw UpdateRefused("not_configured", "a trusted key line is malformed")
                    }
                    TrustedKey(
                        fields[0],
                        fields[1].chunked(2).map { it.toInt(16).toByte() }.toByteArray(),
                    )
                }
                .also { keys ->
                    if (keys.map { it.id }.toSet().size != keys.size)
                        throw UpdateRefused("not_configured", "a trusted key is listed twice")
                }
    }
}

const val NOT_CONFIGURED =
    "this build of LCL trusts no update signing key yet, so it cannot verify, and never installs, any update"

/** The manifest, read only once a trusted key's signature over its exact bytes verified. */
fun verifyManifest(bytes: ByteArray, signature: ByteArray, keys: List<TrustedKey>): UpdateManifest {
    if (keys.isEmpty()) throw UpdateRefused("not_configured", NOT_CONFIGURED)
    val signer =
        keys.firstOrNull { key ->
            runCatching {
                    Signature.getInstance("SHA256withECDSA").run {
                        initVerify(key.key)
                        update(bytes)
                        verify(signature)
                    }
                }
                .getOrDefault(false)
        }
            ?: throw UpdateRefused(
                "verification",
                "the update manifest's signature is not valid for any key this build trusts",
            )
    val manifest = parseManifest(bytes)
    if (manifest.signingKeyId != signer.id) {
        throw UpdateRefused(
            "verification",
            "the manifest names key ${manifest.signingKeyId}, but key ${signer.id} signed it",
        )
    }
    return manifest
}

/** What is installed now. */
data class InstalledApp(
    val versionName: String,
    val versionCode: Long,
    val sdk: Int,
    val signers: Set<String>,
)

/** What an APK file says it is, as Android reads it. */
data class ApkFacts(
    val packageName: String,
    val versionName: String,
    val versionCode: Long,
    val signers: Set<String>,
)

/** Whether the manifest's release is newer than the installed app, and installable here. */
fun isNewer(manifest: UpdateManifest, installed: InstalledApp): Boolean {
    val current = runCatching { SemVer.parse(installed.versionName) }.getOrNull()
    if (current != null && manifest.productVersion <= current) return false
    if (manifest.android.versionCode <= installed.versionCode) {
        if (current == null) return false
        throw UpdateRefused(
            "invalid",
            "LCL ${ProductVersion.shown(manifest.productVersion.toString())} does not advance the app's versionCode",
        )
    }
    if (current != null && current < manifest.minimumSupportedVersion) {
        throw UpdateRefused(
            "unsupported",
            "LCL ${ProductVersion.shown(manifest.productVersion.toString())} replaces " +
                "${ProductVersion.shown(manifest.minimumSupportedVersion.toString())} or newer only; install it manually",
        )
    }
    if (installed.sdk < manifest.android.minimumSdk) {
        throw UpdateRefused(
            "unsupported",
            "LCL ${ProductVersion.shown(manifest.productVersion.toString())} needs Android API ${manifest.android.minimumSdk}",
        )
    }
    return true
}

/**
 * The downloaded APK, [size] bytes with SHA-256 [digest], is exactly the signed one, is this app,
 * is newer and is signed like the installed app.
 */
fun checkApk(
    size: Long,
    digest: String,
    facts: ApkFacts?,
    manifest: UpdateManifest,
    installed: InstalledApp,
) {
    val a = manifest.android
    if (size != a.size || digest != a.sha256) {
        throw UpdateRefused(
            "verification",
            "the downloaded APK does not match the size and SHA-256 its manifest signed",
        )
    }
    facts
        ?: throw UpdateRefused("verification", "the downloaded file is not an APK Android can read")
    if (facts.packageName != APPLICATION_ID)
        throw UpdateRefused("verification", "the APK is ${facts.packageName}, not LCL")
    if (facts.versionCode != a.versionCode || facts.versionName != a.versionName) {
        throw UpdateRefused(
            "verification",
            "the APK is ${facts.versionName} (${facts.versionCode}), not the signed ${a.versionName} (${a.versionCode})",
        )
    }
    if (facts.versionCode <= installed.versionCode)
        throw UpdateRefused("invalid", "the APK is not newer than the app installed")
    if (
        facts.signers.isEmpty() ||
            facts.signers != installed.signers ||
            a.signerSha256 !in facts.signers
    ) {
        throw UpdateRefused(
            "verification",
            "the APK is not signed by the same key as the LCL installed, so Android could not update it in place",
        )
    }
}

/** A release, as the latest-release listing names it. */
data class Release(val tag: String, val assets: Map<String, Long>)

/** The listing's release, if it is published and stable. */
fun parseRelease(text: String): Release? {
    val json =
        runCatching { Json.parseToJsonElement(text) as JsonObject }.getOrNull()
            ?: throw UpdateRefused("invalid", "the release listing is not JSON")
    fun flag(key: String) =
        (json[key] as? JsonPrimitive)?.booleanOrNull
            ?: throw UpdateRefused("invalid", "the release listing has no $key")
    if (flag("draft") || flag("prerelease")) return null
    val tag =
        (json["tag_name"] as? JsonPrimitive)?.contentOrNull?.takeIf {
            Regex("[A-Za-z0-9._-]{1,200}").matches(it)
        } ?: throw UpdateRefused("invalid", "the release listing has no usable tag")
    val assets =
        (json["assets"] as? kotlinx.serialization.json.JsonArray
                ?: throw UpdateRefused("invalid", "the release listing has no assets"))
            .mapNotNull { element: JsonElement ->
                val obj = element as? JsonObject ?: return@mapNotNull null
                val name = (obj["name"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
                val size = (obj["size"] as? JsonPrimitive)?.longOrNull ?: return@mapNotNull null
                name to size
            }
            .toMap()
    return Release(tag, assets)
}
