package io.lcl.workspace

import android.security.keystore.KeyProperties
import io.lcl.workspace.remote.KeyProtection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class KeyProtectionTest {
    @Test
    fun each_level_android_reports_is_named_and_only_hardware_is_called_hardware() {
        for ((level, expected) in listOf(
            KeyProperties.SECURITY_LEVEL_STRONGBOX to KeyProtection.STRONGBOX,
            KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT to KeyProtection.TRUSTED_ENVIRONMENT,
            KeyProperties.SECURITY_LEVEL_UNKNOWN_SECURE to KeyProtection.SECURE_HARDWARE,
            KeyProperties.SECURITY_LEVEL_SOFTWARE to KeyProtection.SOFTWARE,
            KeyProperties.SECURITY_LEVEL_UNKNOWN to KeyProtection.UNKNOWN,
            99 to KeyProtection.UNKNOWN,
        )) {
            // From API 31 the level decides; the older flag is not consulted.
            assertEquals("level $level", expected, KeyProtection.of(level, insideSecureHardware = true))
            assertEquals("level $level", expected, KeyProtection.of(level, insideSecureHardware = false))
        }
        assertTrue(KeyProtection.STRONGBOX.hardwareBacked)
        assertTrue(KeyProtection.TRUSTED_ENVIRONMENT.hardwareBacked)
        assertTrue(KeyProtection.SECURE_HARDWARE.hardwareBacked)
        assertFalse("a software key was called hardware-backed", KeyProtection.SOFTWARE.hardwareBacked)
        assertFalse(KeyProtection.UNKNOWN.hardwareBacked)
    }

    @Test
    fun before_api_31_only_the_secure_hardware_flag_is_known() {
        assertEquals(KeyProtection.SECURE_HARDWARE, KeyProtection.of(null, insideSecureHardware = true))
        assertEquals(KeyProtection.SOFTWARE, KeyProtection.of(null, insideSecureHardware = false))
    }

    @Test
    fun a_store_that_cannot_say_reports_nothing() {
        val identities = FakeIdentities()
        identities.create("lcl-pc-x", "LCL Android")
        assertEquals(null, identities.protection("lcl-pc-x"))
    }
}
