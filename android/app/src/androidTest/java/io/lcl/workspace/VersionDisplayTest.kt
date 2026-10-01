package io.lcl.workspace

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The version people see, on the app built with this release's defaults (versionName 0.9.1,
 * versionCode 9001 derived from it): Updates and About both say "LCL 0.9.1", and neither shows the
 * versionCode. No PC is needed.
 */
@RunWith(AndroidJUnit4::class)
class VersionDisplayTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    private fun exists(tag: String) =
        rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun textOf(tag: String): String =
        rule
            .onNodeWithTag(tag, useUnmergedTree = true)
            .fetchSemanticsNode()
            .config
            .getOrNull(SemanticsProperties.Text)
            ?.joinToString("") { it.text } ?: ""

    private fun back() {
        rule.runOnUiThread { rule.activity.onBackPressedDispatcher.onBackPressed() }
        rule.waitForIdle()
    }

    @Test
    fun updates_and_about_show_lcl_0_5_and_never_the_package_numbers() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val info = context.packageManager.getPackageInfo(context.packageName, 0)
        assertEquals("0.9.1", info.versionName)
        assertEquals(9001L, info.longVersionCode)

        rule.waitUntil("the app is up", 30_000) { exists("home_updates") }
        rule.onNodeWithTag("home_updates").performScrollTo().performClick()
        rule.waitUntil("Updates", 10_000) { exists("update_installed") }
        rule.onNodeWithTag("update_installed").assertTextEquals("Installed version: LCL 0.9.1")
        back()

        rule.waitUntil("home", 10_000) { exists("home_about") }
        rule.onNodeWithTag("home_about").performScrollTo().performClick()
        rule.waitUntil("About", 10_000) { exists("about:App") }
        rule.onNodeWithTag("about:App").assertTextEquals("LCL 0.9.1")
        for (tag in listOf("about:App", "about:PC service")) {
            if (!exists(tag)) continue
            val shown = textOf(tag)
            assertFalse(shown, shown.contains("9001"))
        }
    }
}
