package io.lcl.workspace

import io.lcl.workspace.data.KeyValueStore
import io.lcl.workspace.update.ApkFacts
import io.lcl.workspace.update.ApkInstaller
import io.lcl.workspace.update.InstalledApp
import io.lcl.workspace.update.Offline
import io.lcl.workspace.update.PackageFacts
import io.lcl.workspace.update.Phase
import io.lcl.workspace.update.Release
import io.lcl.workspace.update.ReleaseSource
import io.lcl.workspace.update.SemVer
import io.lcl.workspace.update.TrustedKey
import io.lcl.workspace.update.UpdateController
import io.lcl.workspace.update.UpdateRefused
import io.lcl.workspace.update.copyBounded
import io.lcl.workspace.update.parseManifest
import io.lcl.workspace.update.sha256
import io.lcl.workspace.update.verifyManifest
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.nio.file.Files
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.Signature
import java.security.spec.ECGenParameterSpec

@OptIn(ExperimentalCoroutinesApi::class)
class UpdateTest {
    private fun keyPair(): KeyPair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
    private fun sign(pair: KeyPair, bytes: ByteArray): ByteArray = Signature.getInstance("SHA256withECDSA").run { initSign(pair.private); update(bytes); sign() }
    private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }
    private fun keys(vararg pairs: Pair<String, KeyPair>) = TrustedKey.parse(pairs.joinToString("\n") { (id, pair) -> "$id ${hex(pair.public.encoded)}" })

    private val signerCert = "c".repeat(64)
    // Larger than one download buffer, so it arrives in several pieces.
    private val apk = ByteArray(200_000) { (it * 7).toByte() }

    private fun manifest(version: String, code: Long, apkBytes: ByteArray = apk, keyId: String = "test-key", appId: String = "io.lcl.workspace") = """
        {"format": 1, "product": "lcl", "channel": "stable", "product_version": "$version", "release_tag": "v$version",
         "source_commit": "${"a".repeat(40)}", "published_at": "2026-10-01T12:00:00Z", "release_notes": "Notes for $version",
         "minimum_supported_version": "0.1.0", "signing_key_id": "$keyId",
         "pc": {"artifact_name": "lcl-$version-linux-x86_64.tar.gz", "size": 10, "sha256": "${"b".repeat(64)}",
                "architecture": "x86_64-linux", "required_updater_version": 1},
         "android": {"artifact_name": "lcl-android-$version-$code.apk", "size": ${apkBytes.size}, "sha256": "${sha256(apkBytes)}",
                "application_id": "$appId", "version_name": "$version", "version_code": $code, "minimum_sdk": 29,
                "signer_sha256": "$signerCert"}}
    """.trimIndent().toByteArray()

    @Test
    fun versions_compare_by_semantic_versioning_never_as_text() {
        val ordered = listOf("0.9.0", "0.10.0", "1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0", "1.10.0")
        ordered.zipWithNext().forEach { (a, b) -> assertTrue("$a < $b", SemVer.parse(a) < SemVer.parse(b)) }
        assertEquals(SemVer.parse("1.2.3"), SemVer.parse("1.2.3"))
        // The PC's limits: unsigned 64-bit numbers.
        assertTrue(SemVer.parse("18446744073709551615.0.0") > SemVer.parse("9223372036854775808.0.0"))
        assertTrue(SemVer.parse("1.0.0-18446744073709551615") < SemVer.parse("1.0.0-a"))
        for (bad in listOf("1.2", "01.2.3", "1.2.3+b", "v1.2.3", "1.2.3-", "1.2.3-01", "18446744073709551616.0.0", "1.0.0-18446744073709551616")) {
            assertThrows(UpdateRefused::class.java) { SemVer.parse(bad) }
        }
    }

    @Test
    fun a_manifest_is_read_only_after_a_trusted_signature_verifies() {
        val current = keyPair()
        val next = keyPair()
        val trusted = keys("test-key" to current, "next-key" to next)
        val bytes = manifest("0.2.0", 5)
        val m = verifyManifest(bytes, sign(current, bytes), trusted)
        assertEquals("0.2.0", m.productVersion.toString())
        assertEquals(5L, m.android.versionCode)
        assertEquals("Notes for 0.2.0", m.releaseNotes)
        // Any key the app lists verifies; a key the manifest merely names never
        // does.
        val byNext = manifest("0.2.0", 5, keyId = "next-key")
        assertEquals("next-key", verifyManifest(byNext, sign(next, byNext), trusted).signingKeyId)
        fun refused(kind: String, block: () -> Unit) = assertEquals(kind, assertThrows(UpdateRefused::class.java) { block() }.kind)
        refused("verification") { verifyManifest(bytes, sign(keyPair(), bytes), trusted) }
        refused("verification") { verifyManifest(manifest("0.3.0", 5), sign(current, bytes), trusted) }
        refused("verification") { verifyManifest(bytes, ByteArray(0), trusted) }
        refused("not_configured") { verifyManifest(bytes, sign(current, bytes), emptyList()) }
        val lying = manifest("0.2.0", 5, keyId = "next-key")
        refused("verification") { verifyManifest(lying, sign(current, lying), trusted) }
    }

    @Test
    fun a_malformed_or_unexpected_manifest_is_refused() {
        val good = manifest("0.2.0", 5).toString(Charsets.UTF_8)
        for (bad in listOf(
            good.replace("\"format\": 1", "\"format\": 2"),
            good.replace("\"stable\"", "\"beta\""),
            good.replace("\"product_version\": \"0.2.0\"", "\"product_version\": \"0.2.0-rc.1\""),
            good.replace("io.lcl.workspace", "io.evil.app"),
            good.replace("\"version_code\": 5", "\"version_code\": 0"),
            good.replace("\"format\": 1,", "\"format\": 1, \"exec\": \"rm -rf /\","),
            good.replace("lcl-android-0.2.0-5.apk", "../lcl.apk"),
            "{not json",
        )) {
            assertThrows(bad, UpdateRefused::class.java) { parseManifest(bad.toByteArray()) }
        }
    }

    @Test
    fun the_shared_manifest_vectors_are_accepted_or_refused_exactly_as_on_the_pc() {
        var root = File(".").canonicalFile
        while (!File(root, "update/manifest_vectors/expected.txt").isFile) {
            root = root.parentFile ?: error("no update/manifest_vectors above ${File(".").canonicalPath}")
        }
        val vectors = File(root, "update/manifest_vectors")
        val cases = File(vectors, "expected.txt").readLines().filter { it.isNotEmpty() && !it.startsWith("#") }.map { line ->
            val fields = line.split(" ")
            assertTrue(line, fields.size == 2 && fields[1] in setOf("ACCEPT", "REFUSE"))
            fields[0] to (fields[1] == "ACCEPT")
        }
        assertEquals("every vector is listed exactly once", vectors.list()!!.filter { it != "expected.txt" }.sorted(), cases.map { it.first }.sorted())
        assertTrue(cases.size >= 40)
        for ((name, accept) in cases) {
            val result = runCatching { parseManifest(File(vectors, name).readBytes()) }
            assertEquals("$name: ${result.exceptionOrNull()}", accept, result.isSuccess)
            if (!accept) assertTrue("$name: ${result.exceptionOrNull()}", result.exceptionOrNull() is UpdateRefused)
        }
        val largest = parseManifest(File(vectors, "overflowing-minimum-sdk.json").readBytes().toString(Charsets.UTF_8)
            .replace("4294967325", "2147483647").toByteArray())
        assertEquals(Int.MAX_VALUE, largest.android.minimumSdk)
    }

    @Test
    fun a_download_is_copied_within_its_bounds_or_refused() {
        fun refused(kind: String, block: () -> Unit) = assertEquals(kind, assertThrows(UpdateRefused::class.java) { block() }.kind)
        val bytes = ByteArray(300_000) { it.toByte() }
        val out = ByteArrayOutputStream()
        val seen = mutableListOf<Long>()
        assertEquals(300_000L, copyBounded(Dropping(bytes), out, 300_000, 300_000) { done, _ -> seen += done })
        assertTrue(out.toByteArray().contentEquals(bytes))
        assertTrue("it arrived in pieces", seen.size > 1)
        // Interrupted part way.
        refused("download") { copyBounded(Dropping(bytes, failAfter = 100_000), ByteArrayOutputStream(), 300_000, 300_000) }
        // More than announced by the server, or more than the manifest allows:
        // refused before anything past the limit is written.
        refused("invalid") { copyBounded(Dropping(bytes), ByteArrayOutputStream(), 300_000, 299_999) }
        val capped = ByteArrayOutputStream()
        refused("invalid") { copyBounded(Dropping(bytes), capped, -1, 250_000) }
        assertTrue(capped.size() <= 250_000)
        // Shorter than announced.
        refused("download") { copyBounded(Dropping(bytes.copyOf(299_000)), ByteArrayOutputStream(), 300_000, 300_000) }
    }

    @Test
    fun an_apk_is_streamed_to_the_cache_and_nothing_is_left_when_it_fails() {
        fun cached(s: Setup) = File(s.cache, "update").listFiles().orEmpty().toList()
        fun failed(s: Setup, kind: String) {
            s.controller.check()
            s.controller.update()
            assertEquals(Phase.Failed, s.controller.ui.value.phase)
            assertEquals(kind, s.controller.ui.value.problem!!.first)
            assertTrue("nothing was handed to Android", s.installer.installs.isEmpty())
            assertEquals("nothing is left in the cache", emptyList<File>(), cached(s))
        }
        failed(Setup().apply { source.failAfter = apk.size / 2 }, "download")
        failed(Setup(apkServed = apk.copyOf().also { it[apk.size - 1] = 1 }), "verification")
        failed(Setup(apkServed = apk + ByteArray(10)), "invalid")

        val s = Setup()
        s.controller.check()
        s.controller.update()
        val staged = s.installer.installs.single()
        assertTrue(staged.path.startsWith(s.cache.path))
        assertTrue("the file is exactly the signed APK", staged.readBytes().contentEquals(apk))
        assertEquals(listOf(staged), cached(s))
        assertEquals(Phase.Installing, s.controller.ui.value.phase)
    }

    private class Store : KeyValueStore {
        val values = mutableMapOf<String, String>()
        override fun get(key: String) = values[key]
        override fun put(key: String, value: String?) {
            if (value == null) values.remove(key) else values[key] = value
        }
    }

    /** A stream of [bytes] that fails, as a dropped connection does, once [failAfter] of them are read. */
    private class Dropping(bytes: ByteArray, private val failAfter: Int = Int.MAX_VALUE) : InputStream() {
        private val inner = ByteArrayInputStream(bytes)
        private var read = 0
        override fun read(): Int {
            if (read >= failAfter) throw IOException("connection reset")
            return inner.read().also { if (it >= 0) read++ }
        }
        override fun read(b: ByteArray, off: Int, len: Int): Int {
            if (read >= failAfter) throw IOException("connection reset")
            return inner.read(b, off, minOf(len, failAfter - read, 4096)).also { if (it > 0) read += it }
        }
    }

    /** Serves assets through [copyBounded], as the real source does. */
    private class FakeSource(var release: Release?, val assets: Map<String, ByteArray>, var offline: Boolean = false, var failAfter: Int = Int.MAX_VALUE) : ReleaseSource {
        override fun latest(): Release? = if (offline) throw Offline("no network") else release
        override fun fetch(release: Release, name: String, limit: Long, into: OutputStream, progress: (Long, Long) -> Unit) {
            if (offline) throw Offline("no network")
            val bytes = assets.getValue(name)
            copyBounded(Dropping(bytes, failAfter), into, bytes.size.toLong(), limit, progress)
        }
    }

    private class FakeFacts(var installed: InstalledApp, var archive: ApkFacts?) : PackageFacts {
        override fun installed() = installed
        override fun archive(file: File) = archive
    }

    private class FakeInstaller(var allowed: Boolean = true) : ApkInstaller {
        val installs = mutableListOf<File>()
        var asked = 0
        override fun canInstall() = allowed
        override fun askPermission() { asked++ }
        override fun install(file: File) { installs += file }
    }

    private inner class Setup(version: String = "0.2.0", code: Long = 5, apkServed: ByteArray = apk, archive: ApkFacts? = ApkFacts("io.lcl.workspace", "0.2.0", 5, setOf("c".repeat(64)))) {
        val pair = keyPair()
        val bytes = manifest(version, code)
        val release = Release("v$version", mapOf(
            UpdateController.MANIFEST to bytes.size.toLong(), UpdateController.SIGNATURE to 72L, "lcl-android-$version-$code.apk" to apk.size.toLong()))
        val source = FakeSource(release, mapOf(UpdateController.MANIFEST to bytes, UpdateController.SIGNATURE to sign(pair, bytes), "lcl-android-$version-$code.apk" to apkServed))
        val facts = FakeFacts(InstalledApp("0.1.0", 4, 34, setOf(signerCert)), archive)
        val installer = FakeInstaller()
        val store = Store()
        var now = 1_000_000L
        val cache: File = Files.createTempDirectory("lcl-update-test").toFile()
        val controller = UpdateController(store, source, { keys("test-key" to pair) }, facts, installer,
            cache, TestScope(UnconfinedTestDispatcher()), UnconfinedTestDispatcher(), { now })
    }

    @Test
    fun only_a_newer_release_is_offered_and_checking_needs_no_pc() {
        val s = Setup()
        s.controller.check()
        assertEquals(Phase.Available, s.controller.ui.value.phase)
        assertEquals("0.2.0", s.controller.ui.value.manifest!!.productVersion.toString())
        s.facts.installed = InstalledApp("0.2.0", 5, 34, setOf(signerCert))
        s.controller.check()
        assertEquals("the same version is up to date", Phase.UpToDate, s.controller.ui.value.phase)
        s.facts.installed = InstalledApp("0.3.0", 9, 34, setOf(signerCert))
        s.controller.check()
        assertEquals("an older release is never offered", Phase.UpToDate, s.controller.ui.value.phase)
        s.source.release = null
        s.controller.check()
        assertEquals("no release at all", Phase.UpToDate, s.controller.ui.value.phase)
        s.source.offline = true
        s.controller.check()
        assertEquals(Phase.Offline, s.controller.ui.value.phase)
        assertEquals("offline", s.controller.ui.value.problem!!.first)
    }

    @Test
    fun a_check_happens_at_most_once_a_day() {
        val s = Setup()
        s.controller.checkWhenDue()
        assertEquals(Phase.Available, s.controller.ui.value.phase)
        s.source.offline = true
        s.now += UpdateController.DAY_MS - 1
        s.controller.checkWhenDue()
        assertEquals("not due: nothing asked", Phase.Available, s.controller.ui.value.phase)
        s.now += 1
        s.controller.checkWhenDue()
        assertEquals(Phase.Offline, s.controller.ui.value.phase)
    }

    @Test
    fun an_apk_is_handed_to_android_only_when_it_is_exactly_the_signed_one_signed_like_this_app() {
        fun refusedBeforeInstall(s: Setup, kind: String) {
            s.controller.check()
            s.controller.update()
            assertEquals(Phase.Failed, s.controller.ui.value.phase)
            assertEquals(kind, s.controller.ui.value.problem!!.first)
            assertTrue("nothing was handed to Android", s.installer.installs.isEmpty())
        }
        refusedBeforeInstall(Setup(apkServed = apk.copyOf().also { it[0] = 99 }), "verification")
        refusedBeforeInstall(Setup(apkServed = apk.copyOf(apk.size - 1)), "verification")
        refusedBeforeInstall(Setup(archive = ApkFacts("io.evil.app", "0.2.0", 5, setOf(signerCert))), "verification")
        refusedBeforeInstall(Setup(archive = ApkFacts("io.lcl.workspace", "0.2.0", 5, setOf("d".repeat(64)))), "verification")
        refusedBeforeInstall(Setup(archive = null), "verification")

        val s = Setup()
        s.controller.check()
        s.controller.update()
        assertEquals(1, s.installer.installs.size)
        assertEquals(Phase.Installing, s.controller.ui.value.phase)
    }

    @Test
    fun without_permission_the_person_is_asked_and_the_update_resumes() {
        val s = Setup()
        s.installer.allowed = false
        s.controller.check()
        s.controller.update()
        assertEquals(Phase.NeedsPermission, s.controller.ui.value.phase)
        assertTrue(s.installer.installs.isEmpty())
        s.controller.askPermission()
        assertEquals(1, s.installer.asked)
        s.controller.resume()
        assertTrue("still not allowed: nothing installed", s.installer.installs.isEmpty())
        s.installer.allowed = true
        s.controller.resume()
        assertEquals(1, s.installer.installs.size)
    }

    @Test
    fun the_installers_answers_are_shown_as_they_are() {
        val s = Setup()
        s.controller.check()
        s.controller.update()
        var confirmed = 0
        s.controller.onInstallStatus(UpdateController.STATUS_PENDING_USER_ACTION, null) { confirmed++ }
        assertEquals(1, confirmed)
        assertTrue(s.controller.ui.value.waitingForConfirmation)
        s.controller.onInstallStatus(UpdateController.STATUS_FAILURE_ABORTED, null, null)
        assertEquals(Phase.Failed, s.controller.ui.value.phase)
        assertEquals("cancelled", s.controller.ui.value.problem!!.first)
        assertTrue(s.controller.ui.value.problem!!.second.contains("LCL was not changed"))
        s.controller.update()
        s.controller.onInstallStatus(1, "INSTALL_FAILED_UPDATE_INCOMPATIBLE", null)
        assertEquals("install", s.controller.ui.value.problem!!.first)
        assertTrue(s.controller.ui.value.problem!!.second.contains("LCL was not changed"))
        s.controller.update()
        s.controller.onInstallStatus(UpdateController.STATUS_SUCCESS, null, null)
        assertEquals(Phase.UpToDate, s.controller.ui.value.phase)
        assertFalse(s.controller.ui.value.waitingForConfirmation)
    }

    @Test
    fun a_build_with_no_trusted_key_never_offers_an_update() {
        val s = Setup()
        val controller = UpdateController(s.store, s.source, { emptyList() }, s.facts, s.installer,
            Files.createTempDirectory("lcl-update-test").toFile(), TestScope(UnconfinedTestDispatcher()), UnconfinedTestDispatcher())
        controller.check()
        assertEquals(Phase.NotConfigured, controller.ui.value.phase)
    }
}
