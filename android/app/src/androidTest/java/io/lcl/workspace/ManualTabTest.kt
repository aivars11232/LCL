package io.lcl.workspace

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.lcl.workspace.manual.ManualSnapshot
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Manual tab on a phone with no PC: it opens offline, is the packaged snapshot — the digest the
 * desktop workspace serves, passed as `-e digest` — and survives recreation; the help icon switches
 * to it.
 */
@RunWith(AndroidJUnit4::class)
class ManualTabTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    private fun exists(tag: String) =
        rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun status(): String = runCatching {
        rule
            .onNodeWithTag("manual_status", useUnmergedTree = true)
            .fetchSemanticsNode()
            .config
            .getOrNull(SemanticsProperties.Text)
            ?.joinToString("") { it.text } ?: ""
    }
        .getOrDefault("")

    private fun waitForTheManual() =
        rule.waitUntil("the manual is shown, offline", 30_000) { status().contains("offline") }

    @Test
    fun the_manual_opens_offline_is_the_packaged_snapshot_and_survives_recreation() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val assets = context.assets
        val snapshot =
            ManualSnapshot.load(assets.list("manual")!!.toList()) { name ->
                assets.open("manual/$name").use { it.readBytes() }
            }
        assertTrue("the packaged manual is not its manifest's snapshot", snapshot.matchesManifest)
        InstrumentationRegistry.getArguments().getString("digest")?.let { desktop ->
            assertEquals("the phone's manual is not the desktop's", desktop, snapshot.digest)
        }

        rule.waitUntil("the app is up", 30_000) { exists("tab_manual") }
        rule.onNodeWithTag("tab_manual").performClick()
        waitForTheManual()
        assertTrue(status(), status().contains(snapshot.digest.take(12)))
        assertTrue(status(), status().contains("${snapshot.files.size} files"))

        // Rotation and other recreation keep the Manual tab.
        rule.activityRule.scenario.recreate()
        waitForTheManual()

        // Back to Workspace, then the help icon opens the Manual again.
        rule.onNodeWithTag("tab_workspace").performClick()
        rule.waitUntil("the workspace tab", 10_000) {
            exists("manual_icon") && !exists("manual_status")
        }
        rule.onNodeWithTag("manual_icon").performClick()
        waitForTheManual()
    }
}
