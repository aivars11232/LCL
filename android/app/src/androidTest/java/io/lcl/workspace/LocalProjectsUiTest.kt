package io.lcl.workspace

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * A project on this phone, with no PC paired at all: made from the
 * dashboard, a folder and a document made in it, the document edited and
 * saved, the engine's actions unavailable, everything on the app's private
 * storage and still there after the activity is recreated.
 */
@RunWith(AndroidJUnit4::class)
class LocalProjectsUiTest {
    @get:Rule
    val rule = createAndroidComposeRule<MainActivity>()

    private fun exists(tag: String) = rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun waitFor(tag: String) = rule.waitUntil(tag, 15_000) { exists(tag) }
    private fun textOf(tag: String): String =
        rule.onNodeWithTag(tag, useUnmergedTree = true).fetchSemanticsNode().config.getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text } ?: ""
    private fun source(): String =
        rule.onNodeWithTag("source", useUnmergedTree = true).fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text ?: ""

    @Test
    fun a_project_on_the_phone_is_made_edited_and_saved_without_a_pc() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val storage = File(context.filesDir, "local-projects")
        File(storage, "Offline").deleteRecursively()
        waitFor("new_local_project")
        rule.onNodeWithTag("new_local_project").performScrollTo().performClick()
        rule.onNodeWithTag("local_project_name").performTextInput("Offline")
        rule.onNodeWithTag("create_local_project").performClick()
        // The workspace opens on it, usable with no PC.
        waitFor("project_where")
        assertTrue(textOf("project_where"), textOf("project_where").startsWith("On this phone"))
        rule.onNodeWithTag("new_folder").assertIsEnabled().performClick()
        rule.onNodeWithTag("new_folder_name").performTextInput("planning")
        rule.onNodeWithTag("create_folder").performClick()
        waitFor("file:planning")
        rule.onNodeWithTag("new_document").performClick()
        rule.onNodeWithTag("new_name").performTextClearance()
        rule.onNodeWithTag("new_name").performTextInput("planning/phase_1")
        rule.onNodeWithTag("create").performClick()
        waitFor("source")
        assertTrue(File(storage, "Offline/planning/phase_1.lcl").isFile)
        // Saved on the phone; the engine's actions need a PC and are off.
        rule.onNodeWithTag("source").performTextInput("PHONE")
        rule.waitUntil("unsaved", 5_000) { textOf("doc_state").startsWith("Unsaved") }
        rule.onNodeWithTag("action_save").assertIsEnabled().performClick()
        rule.waitUntil("saved", 10_000) { textOf("doc_state").startsWith("Saved") }
        val onDisk = File(storage, "Offline/planning/phase_1.lcl").readText()
        assertEquals(source(), onDisk)
        assertTrue(onDisk.contains("PHONE"))
        for (tag in listOf("action_check", "action_run", "action_inspect", "action_validate")) rule.onNodeWithTag(tag).assertIsNotEnabled()
        // Recreated (rotation, memory pressure): the project and the document are still there.
        rule.activityRule.scenario.recreate()
        rule.waitUntil("the app is back", 15_000) { exists("new_local_project") || exists("project_where") || exists("source") }
        if (exists("new_local_project")) {
            rule.onNodeWithTag("open_local:Offline").performScrollTo().performClick()
            // The document is still open, so the editor shows it again.
            waitFor("source")
            assertTrue(source(), source().contains("PHONE"))
        }
        assertFalse(File(storage, "Offline/planning/.phase_1.lcl.tmp").exists())
        assertTrue(File(storage, "Offline/planning/phase_1.lcl").readText().contains("PHONE"))
        File(storage, "Offline").deleteRecursively()
    }
}
