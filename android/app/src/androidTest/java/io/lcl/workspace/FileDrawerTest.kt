package io.lcl.workspace

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.lcl.workspace.ui.FilesDrawer
import io.lcl.workspace.ui.ProjectTree
import io.lcl.workspace.ui.TREE_TRUNCATED
import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.TreeEntry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Workspace's project tree drawer, as the screen composes it, over a
 * listing the test holds in place of a PC's: it opens from its button, folds
 * folders, opens a file and closes, marks the file being edited, follows a
 * newer listing, and closes on a tap outside it or on Back. No PC is needed.
 */
@RunWith(AndroidJUnit4::class)
class FileDrawerTest {
    @get:Rule
    val rule = createAndroidComposeRule<ComponentActivity>()

    private val listing = listOf(TreeEntry("a.lcl", false), TreeEntry("docs", true), TreeEntry("docs/g.lcl", false))
    private var entries by mutableStateOf(listing)
    private var folded by mutableStateOf(emptySet<String>())
    private var active by mutableStateOf<String?>(null)
    private var truncated by mutableStateOf(false)

    private fun exists(tag: String) = rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    /** Whether the merged tree, the one accessibility services read, has the node. */
    private fun seen(tag: String) = rule.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    private fun show() = rule.setContent {
        MaterialTheme {
            FilesDrawer(
                rememberDrawerState(DrawerValue.Closed),
                width = 300.dp,
                files = { close ->
                    ProjectTree(
                        FileTree.rows(entries, folded),
                        active = active,
                        open = setOfNotNull(active),
                        dirty = emptySet(),
                        connected = true,
                        onToggle = { folded = FileTree.toggle(folded, it) },
                        onOpen = { active = it; close() },
                        onReadiness = {},
                        truncated = truncated,
                    )
                },
            ) { open ->
                Column {
                    TextButton(onClick = open, modifier = Modifier.testTag("files")) { Text("Files") }
                    Text(active ?: "nothing", Modifier.testTag("editing"))
                }
            }
        }
    }

    @Test
    fun the_drawer_opens_folds_opens_a_file_and_closes() {
        show()
        // Closed, nothing of the tree can be found or pressed.
        assertFalse(exists("file:a.lcl"))
        rule.onNodeWithTag("files").performClick()
        rule.onNodeWithTag("file:a.lcl").assertIsDisplayed()
        rule.onNodeWithTag("file:docs/g.lcl").assertIsDisplayed()
        // Open, it is modal: accessibility services no longer find what is behind it.
        assertFalse(seen("files"))
        assertFalse(seen("editing"))

        // A folder folds and unfolds.
        rule.onNodeWithTag("file:docs").performClick()
        assertFalse(exists("file:docs/g.lcl"))
        rule.onNodeWithTag("file:docs").performClick()
        rule.onNodeWithTag("file:docs/g.lcl").assertIsDisplayed()

        // Choosing a file opens it and closes the drawer.
        rule.onNodeWithTag("file:docs/g.lcl").performClick()
        rule.onNodeWithTag("editing").assertTextEquals("docs/g.lcl")
        assertFalse(exists("file:a.lcl"))
        assertTrue(seen("files"))

        // Reopened: the file being edited is marked, and a newer listing shows.
        entries = listing + TreeEntry("new.lcl", false)
        rule.onNodeWithTag("files").performClick()
        rule.onNodeWithTag("file:docs/g.lcl").assertIsSelected()
        rule.onNodeWithTag("file:a.lcl").assertIsNotSelected()
        rule.onNodeWithTag("file:new.lcl").assertIsDisplayed()

        // A tap outside it closes it.
        rule.onRoot().performTouchInput { click(centerRight - Offset(24f, 0f)) }
        rule.waitForIdle()
        assertFalse(exists("file:a.lcl"))
    }

    @Test
    fun a_listing_cut_short_by_the_pc_says_so_in_the_drawer() {
        show()
        rule.onNodeWithTag("files").performClick()
        rule.onNodeWithTag("file:a.lcl").assertIsDisplayed()
        assertFalse(exists("tree_truncated"))
        truncated = true
        rule.onNodeWithTag("tree_truncated").assertIsDisplayed().assertTextEquals(TREE_TRUNCATED)
        // What is listed still opens.
        rule.onNodeWithTag("file:a.lcl").performClick()
        rule.onNodeWithTag("editing").assertTextEquals("a.lcl")
    }

    @Test
    fun back_closes_the_drawer() {
        show()
        rule.onNodeWithTag("files").performClick()
        rule.onNodeWithTag("file:a.lcl").assertIsDisplayed()
        rule.runOnUiThread { rule.activity.onBackPressedDispatcher.onBackPressed() }
        rule.waitForIdle()
        assertFalse(exists("file:a.lcl"))
        rule.onNodeWithTag("editing").assertIsDisplayed()
    }
}
