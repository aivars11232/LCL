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
import io.lcl.workspace.workspace.Explorer
import io.lcl.workspace.workspace.FileTree
import io.lcl.workspace.workspace.LoadedFolder
import io.lcl.workspace.workspace.TreeEntry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Workspace's project tree drawer, as the screen composes it, over an explorer the test holds
 * in place of a PC's: it opens from its button, unfolds and folds folders (a folder's children
 * appear only once it is unfolded), opens a file and closes, marks the file being edited, follows a
 * newer listing, shows an empty folder, notes a folder the PC cut short, and closes on a tap
 * outside it or on Back. No PC is needed.
 */
@RunWith(AndroidJUnit4::class)
class FileDrawerTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    /** The PC's listings: the root and, once asked for, docs and the empty planning folder. */
    private val listing =
        mapOf(
            "" to
                LoadedFolder(
                    listOf(
                        TreeEntry("docs", true),
                        TreeEntry("planning", true),
                        TreeEntry("a.lcl", false),
                    )
                ),
            "docs" to LoadedFolder(listOf(TreeEntry("docs/g.lcl", false))),
            "planning" to LoadedFolder(emptyList()),
        )
    private var explorer by mutableStateOf(Explorer(folders = mapOf("" to listing.getValue(""))))
    private var active by mutableStateOf<String?>(null)
    /** Which folders the tree asked the PC for, in order. */
    private val asked = mutableListOf<String>()

    private fun exists(tag: String) =
        rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    /** Whether the merged tree, the one accessibility services read, has the node. */
    private fun seen(tag: String) = rule.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    private fun show() = rule.setContent {
        MaterialTheme {
            FilesDrawer(
                rememberDrawerState(DrawerValue.Closed),
                width = 300.dp,
                files = { close ->
                    ProjectTree(
                        FileTree.rows(explorer),
                        active = active,
                        open = setOfNotNull(active),
                        dirty = emptySet(),
                        connected = true,
                        onToggle = { folder ->
                            explorer =
                                if (folder in explorer.expanded)
                                    explorer.copy(expanded = explorer.expanded - folder)
                                else {
                                    asked += folder
                                    explorer
                                        .with(folder, listing.getValue(folder))
                                        .copy(expanded = explorer.expanded + folder)
                                }
                        },
                        onOpen = {
                            active = it
                            close()
                        },
                        onReadiness = {},
                    )
                },
            ) { open ->
                Column {
                    TextButton(onClick = open, modifier = Modifier.testTag("files")) {
                        Text("Files")
                    }
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
        // The root alone was listed: a folder's children are not there until
        // it is unfolded, and an empty folder is a folder like any other.
        assertFalse(exists("file:docs/g.lcl"))
        rule.onNodeWithTag("file:planning").assertIsDisplayed()
        assertEquals(emptyList<String>(), asked)
        // Open, it is modal: accessibility services no longer find what is behind it.
        assertFalse(seen("files"))
        assertFalse(seen("editing"))

        // A folder unfolds, asking the PC for its children, and folds.
        rule.onNodeWithTag("file:docs").performClick()
        rule.onNodeWithTag("file:docs/g.lcl").assertIsDisplayed()
        assertEquals(listOf("docs"), asked)
        rule.onNodeWithTag("file:docs").performClick()
        assertFalse(exists("file:docs/g.lcl"))
        rule.onNodeWithTag("file:docs").performClick()
        rule.onNodeWithTag("file:docs/g.lcl").assertIsDisplayed()
        // The empty folder unfolds to nothing, and folds again.
        rule.onNodeWithTag("file:planning").performClick()
        assertEquals(listOf("docs", "docs", "planning"), asked)
        rule.onNodeWithTag("file:planning").performClick()

        // Choosing a file opens it and closes the drawer.
        rule.onNodeWithTag("file:docs/g.lcl").performClick()
        rule.onNodeWithTag("editing").assertTextEquals("docs/g.lcl")
        assertFalse(exists("file:a.lcl"))
        assertTrue(seen("files"))

        // Reopened: the file being edited is marked, and a newer listing shows.
        explorer =
            explorer.with(
                "",
                LoadedFolder(listing.getValue("").entries + TreeEntry("new.lcl", false)),
            )
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
    fun a_folder_cut_short_by_the_pc_says_so_under_that_folder_alone() {
        show()
        rule.onNodeWithTag("files").performClick()
        rule.onNodeWithTag("file:a.lcl").assertIsDisplayed()
        assertFalse(exists("limited:"))
        explorer =
            explorer
                .with("docs", LoadedFolder(listing.getValue("docs").entries, truncated = true))
                .copy(expanded = setOf("docs"))
        rule
            .onNodeWithTag("limited:docs")
            .assertIsDisplayed()
            .assertTextEquals(FileTree.FOLDER_LIMITED)
        assertFalse(exists("limited:"))
        // What is listed still opens.
        rule.onNodeWithTag("file:docs/g.lcl").performClick()
        rule.onNodeWithTag("editing").assertTextEquals("docs/g.lcl")
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
