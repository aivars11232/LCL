package io.lcl.workspace

import io.lcl.workspace.update.ProductVersion
import io.lcl.workspace.update.SemVer
import io.lcl.workspace.update.UpdateRefused
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ProductVersionTest {
    @Test
    fun a_release_reads_without_one_trailing_zero_and_nothing_else_is_shortened() {
        for ((full, shown) in listOf(
            "0.5.0" to "0.5", "1.2.0" to "1.2", "1.0.0" to "1.0", "1.2.3" to "1.2.3",
            "0.10.0" to "0.10", "1.0.0-rc.1" to "1.0.0-rc.1", "0.5" to "0.5", "" to "",
        )) assertEquals(full, shown, ProductVersion.shown(full))
        assertEquals("lcl-remote 0.5", ProductVersion.shownService("lcl-remote 0.5.0"))
        assertEquals("lcl-remote 1.2.3", ProductVersion.shownService("lcl-remote 1.2.3"))
        assertEquals("lcl-remote", ProductVersion.shownService("lcl-remote"))
    }

    @Test
    fun the_published_app_accepts_and_orders_the_next_release() {
        // The app published as 0.1.1 carries this parser: 0.5.0 is newer, and
        // 0.5 is no version at all.
        assertTrue(SemVer.parse("0.5.0") > SemVer.parse("0.1.1"))
        assertThrows(UpdateRefused::class.java) { SemVer.parse("0.5") }
    }

    /** The first `version = "..."` of a Cargo manifest in this repository. */
    private fun cargoVersion(path: String): String =
        File("../../$path").readLines().first { it.startsWith("version = \"") }.removePrefix("version = \"").removeSuffix("\"")

    @Test
    fun the_default_build_is_the_product_version_every_binary_reports() {
        val product = cargoVersion("impl/Cargo.toml")
        assertEquals(product, cargoVersion("remote/Cargo.toml"))
        assertEquals(product, cargoVersion("update/Cargo.toml"))
        assertEquals(product, BuildConfig.VERSION_NAME)
        // v0.1.1 was published with versionCode 7; the default is past it.
        assertTrue("versionCode ${BuildConfig.VERSION_CODE}", BuildConfig.VERSION_CODE > 7)
    }
}
