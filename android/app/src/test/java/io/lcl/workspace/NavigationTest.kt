package io.lcl.workspace

import io.lcl.workspace.ui.Screen
import io.lcl.workspace.ui.backFrom
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class NavigationTest {
    private val screens = listOf(Screen.Home, Screen.Pair, Screen.Workspace, Screen.Settings, Screen.About, Screen.Updates)

    @Test
    fun back_from_pairing_returns_to_the_dashboard_whether_or_not_a_pc_is_in_use() {
        for (pcInUse in listOf(false, true)) {
            assertEquals(Screen.Home, backFrom(Screen.Pair, pcInUse))
        }
    }

    @Test
    fun after_connecting_back_from_the_workspace_is_the_dashboard_not_the_end_of_the_app() {
        assertEquals(Screen.Home, backFrom(Screen.Workspace, pcInUse = true))
        assertEquals(Screen.Home, backFrom(Screen.Workspace, pcInUse = false))
    }

    @Test
    fun only_the_dashboard_leaves_the_app() {
        for (pcInUse in listOf(false, true)) {
            assertNull(backFrom(Screen.Home, pcInUse))
            for (screen in screens - Screen.Home) {
                assertTrue("$screen leaves the app", backFrom(screen, pcInUse) != null)
            }
        }
    }

    @Test
    fun settings_and_about_keep_returning_to_the_workspace_while_a_pc_is_in_use() {
        for (screen in listOf(Screen.Settings, Screen.About)) {
            assertEquals(Screen.Workspace, backFrom(screen, pcInUse = true))
            assertEquals(Screen.Home, backFrom(screen, pcInUse = false))
        }
    }

    /** Pressing Back again and again reaches the dashboard and then leaves,
     *  never visiting a screen twice: nothing traps the person or reopens. */
    @Test
    fun every_back_chain_ends_at_the_dashboard_without_a_loop() {
        for (pcInUse in listOf(false, true)) {
            for (start in screens) {
                val seen = mutableListOf(start)
                var at: Screen? = backFrom(start, pcInUse)
                while (at != null) {
                    assertTrue("$start loops through $seen", at !in seen)
                    seen += at
                    at = backFrom(at, pcInUse)
                }
                assertEquals(Screen.Home, seen.last())
            }
        }
    }
}
