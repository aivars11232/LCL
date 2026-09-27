package io.lcl.workspace

import io.lcl.workspace.manual.ManualSnapshot
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Manual parity: the manual this app packages (the build's generated
 * `manual` assets) is exactly the snapshot `users_manual/MANIFEST.json`
 * names — the same version and digest the desktop workspace serves.
 */
class ManualSnapshotTest {
    private val dir = File(System.getProperty("lcl.manualAssets") ?: error("lcl.manualAssets is not set"))

    private fun packaged() = ManualSnapshot.load(dir.list()!!.toList()) { File(dir, it).readBytes() }

    @Test
    fun the_packaged_manual_is_the_manifest_snapshot() {
        val snapshot = packaged()
        assertTrue("no manual files were packaged", snapshot.files.size > 10)
        assertEquals(snapshot.manifestDigest, snapshot.digest)
        assertTrue(snapshot.matchesManifest)
        assertEquals("0.3.0", snapshot.version)
    }

    @Test
    fun the_packaged_files_are_the_repository_manual_byte_for_byte() {
        val source = File(dir, "../../../../../../users_manual").canonicalFile
        val expected = source.list()!!.filter { it.endsWith(".md") && File(source, it).isFile }.sorted()
        val snapshot = packaged()
        assertEquals(expected, snapshot.files.map { it.first })
        for ((name, bytes) in snapshot.files) assertTrue(name, bytes.contentEquals(File(source, name).readBytes()))
        // The viewer is the desktop workspace's own.
        val viewer = File(source, "../impl/crates/lcl-workspace/assets/manual").canonicalFile
        for (name in listOf("manual.html", "manual.js", "manual.css")) {
            assertTrue(name, File(dir, name).readBytes().contentEquals(File(viewer, name).readBytes()))
        }
    }

    @Test
    fun the_digest_rule_is_the_shared_one() {
        // name, NUL, decimal length, NUL, bytes — for each file in order.
        val files = listOf("a.md" to "x".toByteArray(), "b.md" to ByteArray(0))
        val manual = java.security.MessageDigest.getInstance("SHA-256")
            .digest("a.md\u00001\u0000xb.md\u00000\u0000".toByteArray())
            .joinToString("") { "%02x".format(it) }
        assertEquals(manual, ManualSnapshot.digest(files))
    }

    @Test
    fun the_viewer_json_carries_every_file() {
        val json = packaged().json()
        assertTrue(json.contains("\"digest\""))
        assertTrue(json.contains("01_What_Is_LCL.md"))
    }
}
