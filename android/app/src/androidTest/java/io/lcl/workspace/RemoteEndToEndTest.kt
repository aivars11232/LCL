package io.lcl.workspace

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.security.keystore.KeyInfo
import android.util.Log
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextInputSelection
import androidx.compose.ui.text.TextRange
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.lcl.workspace.remote.PairingLink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyFactory
import java.security.KeyStore
import java.security.PrivateKey

/**
 * The app against a real `lcl-remote serve` on the host, reached from the
 * emulator at 10.0.2.2: real Android Keystore, real TLS, the PC's real engine.
 *
 * Each test is one phase. The host script (android/tools/e2e.sh) runs them in
 * order and does what only the host can between them: restart the app or the
 * emulator, stop the service, edit or check files on the PC, revoke the
 * device. Arguments come as `-e name value`: `link`, `fp`, `workdir`, `oldlink`.
 */
@RunWith(AndroidJUnit4::class)
class RemoteEndToEndTest {
    @get:Rule
    val rule = createAndroidComposeRule<MainActivity>()

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val args get() = InstrumentationRegistry.getArguments()
    private fun arg(name: String): String = args.getString(name) ?: error("pass -e $name")

    private fun exists(tag: String) = rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun waitFor(tag: String, timeout: Long = 30_000) = rule.waitUntil("$tag appears", timeout) { exists(tag) }

    private fun textOf(tag: String): String =
        rule.onNodeWithTag(tag, useUnmergedTree = true).fetchSemanticsNode().config
            .getOrNull(SemanticsProperties.Text)?.joinToString("") { it.text } ?: ""

    private fun label(): String = runCatching { textOf("connection_label") }.getOrDefault("")

    private fun waitForLabel(expected: String, timeout: Long = 60_000) =
        rule.waitUntil("connection is $expected", timeout) { label() == expected }

    private fun source(): String =
        rule.onNodeWithTag("source").fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text ?: ""

    /** A button in the editor's action row, which scrolls sideways on a phone. */
    private fun action(name: String) = rule.onNodeWithTag("action_$name").performScrollTo().performClick()

    /** Tell the host script this phase is ready for its part. */
    private fun ready(marker: String) = Log.i(TAG, marker)

    /** A screenshot into the app's own external files, for the host to pull. */
    private fun shot(name: String) {
        rule.waitForIdle()
        val bitmap = instrumentation.uiAutomation.takeScreenshot() ?: return
        val dir = File(instrumentation.targetContext.getExternalFilesDir(null), "shots").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        Log.i(TAG, "screenshot $name")
    }

    private fun deviceKeys(): List<String> {
        val keystore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        return keystore.aliases().toList().filter { it.startsWith("lcl-pc-") }
    }

    private fun openTodo() {
        if (!exists("source")) {
            if (exists("files")) rule.onNodeWithTag("files").performClick()
            waitFor("file:todo.lcl")
            rule.onNodeWithTag("file:todo.lcl").performClick()
        }
        waitFor("source")
        rule.waitUntil("the document text is shown", 15_000) { source().contains("SPECIFICATION:") }
    }

    /** Type at the end of the document's NAME value, as a person would. */
    private fun typeIntoName(text: String) {
        val current = source()
        val at = current.indexOf("\"", current.indexOf("NAME: \"") + "NAME: \"".length)
        rule.onNodeWithTag("source").performTextInputSelection(TextRange(at))
        rule.onNodeWithTag("source").performTextInput(text)
        rule.waitUntil("the edit is on screen", 5_000) { source().contains("$text\"") }
    }

    private fun goHome() {
        if (exists("pair_new")) return
        if (exists("files")) rule.onNodeWithTag("files").performClick()
        waitFor("home")
        rule.onNodeWithTag("home").performClick()
        waitFor("pair_new")
    }

    private fun pairWith(link: String) {
        goHome()
        rule.onNodeWithTag("pair_new").performClick()
        rule.onNodeWithTag("pair_link").performTextClearance()
        rule.onNodeWithTag("pair_link").performTextInput(link)
        waitFor("pair_preview")
    }

    private fun run(grant: String, answer: String): String {
        action("run")
        waitFor("grant_read")
        rule.onNodeWithTag("grant_read").performTextInput(grant)
        rule.onNodeWithTag("grant_write").performTextInput(grant)
        rule.onNodeWithTag("run_start").performClick()
        var answered = 0
        rule.waitUntil("the run finishes", 120_000) {
            if (exists("approve_$answer")) {
                if (answered == 0) shot("run_${answer}_approval")
                runCatching { rule.onNodeWithTag("approve_$answer").performClick() }.onSuccess { answered += 1 }
            }
            exists("run_state") && textOf("run_state").endsWith("finished")
        }
        val status = if (exists("terminal_status")) textOf("terminal_status") else "(no completion)"
        Log.i(TAG, "run answered $answer $answered time(s): $status")
        assertTrue("the run never asked this device", answered >= 1)
        return status
    }

    /** Pair from a link and stop: for pairing with the code the PC's own window shows. */
    @Test
    fun p0_pair_only() {
        waitFor("pair_new")
        pairWith(arg("link"))
        shot("p0_01_pair_confirm")
        rule.onNodeWithTag("pair_button").performClick()
        waitForLabel("Connected")
        shot("p0_02_paired")
        ready("PAIRED")
    }

    /** Phase 1: pair from a clean install, then edit, save, check, validate, inspect and run. */
    @Test
    fun p1_pair_and_work() {
        val link = arg("link")
        val fingerprint = arg("fp")
        waitFor("pair_new")
        shot("p1_01_home_unpaired")
        // The link arrives from outside the app, as from the phone's camera app.
        // It only fills the form in: nothing is trusted until Pair is pressed.
        instrumentation.targetContext.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(link)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        waitFor("pair_preview")
        Thread.sleep(2_000)
        val shown = "Fingerprint " + fingerprint.chunked(4).take(8).joinToString(" ")
        rule.onNode(hasText(shown, substring = true), useUnmergedTree = true).assertExists()
        assertTrue("an outside link paired without the person", deviceKeys().isEmpty())
        shot("p1_02_pair_confirm")
        rule.onNodeWithTag("pair_button").performClick()
        waitForLabel("Connected")

        // The device key: in Android Keystore, one per PC, never exportable.
        val keys = deviceKeys()
        assertEquals(1, keys.size)
        val keystore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val key = keystore.getKey(keys.single(), null) as PrivateKey
        assertNull("the device's private key is exportable", key.encoded)
        val info = KeyFactory.getInstance(key.algorithm, "AndroidKeyStore").getKeySpec(key, KeyInfo::class.java)
        val level = if (Build.VERSION.SDK_INT >= 31) "securityLevel=${info.securityLevel}" else legacyLevel(info)
        Log.i(TAG, "device key ${keys.single()}: ${key.algorithm}, $level")
        // Nothing reusable is written in plain files: no code, no key.
        val code = PairingLink.parse(link).code
        val preferences = File(instrumentation.targetContext.dataDir, "shared_prefs").listFiles().orEmpty()
        for (file in preferences) {
            val text = file.readText()
            assertFalse("${file.name} holds the pairing code", text.contains(code))
            assertFalse("${file.name} holds a private key", text.contains("PRIVATE KEY"))
        }

        waitFor("file:todo.lcl")
        waitFor("file:notes.lcl.txt") // both endings are LCL documents
        shot("p1_03_files")
        openTodo()
        assertTrue(exists("gutter"))
        Thread.sleep(1_500) // the PC's tokens arrive; the screenshot shows them
        shot("p1_04_editor")

        typeIntoName(" (phone)")
        assertTrue(textOf("doc_state"), textOf("doc_state").startsWith("Unsaved"))
        action("save")
        rule.waitUntil("the PC acknowledges the save", 20_000) { textOf("doc_state").startsWith("Saved") }
        shot("p1_05_saved")

        action("check")
        waitFor("outcome")
        rule.waitUntil("the check report", 20_000) { textOf("outcome").startsWith("Check:") }
        Log.i(TAG, "check: " + textOf("outcome"))
        assertTrue(textOf("outcome"), textOf("outcome").startsWith("Check: accepted"))
        shot("p1_06_check")
        action("validate")
        rule.waitUntil("the validate report", 20_000) { textOf("outcome").startsWith("Validate:") }
        Log.i(TAG, "validate: " + textOf("outcome"))
        action("inspect")
        waitFor("plan")
        Log.i(TAG, "inspect: " + textOf("plan"))
        shot("p1_07_inspect")

        val status = run(arg("workdir"), "allow")
        shot("p1_08_run_allowed")
        assertEquals("status.succeeded", status)
    }

    private fun openSettings() {
        goHome()
        rule.onNodeWithTag("home_settings").performScrollTo().performClick()
        waitFor("line_numbers")
    }

    private fun back() = rule.onNodeWithTag("back").performClick()

    /**
     * Phase 2: a fresh process — the app was closed, or the phone restarted —
     * reconnects with no QR code. About shows the PC engine's specification
     * identities. Settings stay on the phone across restarts. Disconnect keeps
     * the pairing. `-e settings set` changes the settings; `verify` checks they
     * survived and puts them back.
     */
    @Test
    fun p2_restart_reconnects_then_disconnect() {
        waitForLabel("Connected")
        assertFalse("the pairing screen came back", exists("pair_link"))
        assertEquals(1, deviceKeys().size)
        shot("p2_01_reconnected")
        openTodo()
        assertTrue(source().contains("to-do list (phone)\""))

        // About: the identities come from the PC's engine, not from this app.
        goHome()
        rule.onNodeWithTag("home_about").performScrollTo().performClick()
        rule.waitUntil("the PC's engine reports its specifications", 20_000) {
            exists("about:LCL Core 0.1") && textOf("about:LCL Core 0.1").contains(arg("core01"))
        }
        assertTrue(textOf("about:LCL Core 0.2"), textOf("about:LCL Core 0.2").contains(arg("core02")))
        assertEquals("lcl.remote/1", textOf("about:Remote protocol"))
        Log.i(TAG, "about: ${textOf("about:LCL Core 0.1")} | ${textOf("about:LCL Core 0.2")} | ${textOf("about:PC service")}")
        shot("p2_02_about")
        back()

        when (args.getString("settings")) {
            "set" -> {
                openSettings()
                rule.onNodeWithTag("theme_dark").performClick()
                rule.onNodeWithTag("font_size").performSemanticsAction(SemanticsActions.SetProgress) { it(18f) }
                rule.onNodeWithTag("line_numbers").performClick()
                shot("p2_03_settings")
                back()
                openTodo()
                assertFalse("line numbers still shown after turning them off", exists("gutter"))
                shot("p2_04_dark_no_line_numbers")
            }
            "verify" -> {
                openTodo()
                assertFalse("the line-number setting did not survive", exists("gutter"))
                openSettings()
                rule.onNodeWithTag("theme_system").performClick()
                rule.onNodeWithTag("font_size").performSemanticsAction(SemanticsActions.SetProgress) { it(14f) }
                rule.onNodeWithTag("line_numbers").performClick()
                back()
                openTodo()
                assertTrue("line numbers did not come back", exists("gutter"))
            }
        }

        goHome()
        waitFor("disconnect")
        rule.onNodeWithTag("disconnect").performClick()
        waitForLabel("Offline", 10_000)
        shot("p2_05_disconnected")
        Thread.sleep(5_000)
        assertEquals("Disconnect did not hold", "Offline", label())
        rule.onNodeWithTag("reconnect").performClick()
        waitForLabel("Connected")
        shot("p2_06_reconnected")
    }

    /** Phase 3: the network goes away and comes back; the connection follows. */
    @Test
    fun p3_network_loss_and_return() {
        waitForLabel("Connected")
        val shell = instrumentation.uiAutomation
        fun sh(command: String) = shell.executeShellCommand(command).close()
        try {
            sh("cmd connectivity airplane-mode enable")
            rule.waitUntil("the app notices", 60_000) { label() != "Connected" }
            Log.i(TAG, "without network: " + label())
            shot("p3_01_no_network")
        } finally {
            sh("cmd connectivity airplane-mode disable")
        }
        waitForLabel("Connected", 120_000)
        shot("p3_02_back")
    }

    /** Phase 4: the PC's service stops and starts again (the host does it); the app reconnects by itself. */
    @Test
    fun p4_pc_restart() {
        waitForLabel("Connected")
        ready("READY_FOR_PC_RESTART")
        rule.waitUntil("the PC goes away", 90_000) { label() != "Connected" }
        shot("p4_01_pc_gone")
        waitForLabel("Connected", 180_000)
        shot("p4_02_pc_back")
        openTodo()
    }

    /**
     * Phase 5: edits made on the PC reach the phone; a conflicting one stops
     * the phone's save until the person decides.
     */
    @Test
    fun p5_pc_edits_and_conflict() {
        waitForLabel("Connected")
        openTodo()
        ready("READY_FOR_PC_EDIT")
        rule.waitUntil("the PC's edit reaches the phone", 60_000) { source().contains("(phone, pc)\"") }
        assertTrue(textOf("doc_state").startsWith("Saved"))
        shot("p5_01_pc_edit_arrived")

        typeIntoName(" [phone2]")
        ready("READY_FOR_CONFLICT")
        waitFor("conflict_keep", 60_000)
        assertTrue("the phone's edit was lost", source().contains("[phone2]\""))
        shot("p5_02_conflict")
        rule.onNodeWithTag("conflict_keep").performClick()
        action("save")
        rule.waitUntil("the phone's version is saved on purpose", 20_000) { textOf("doc_state").startsWith("Saved") }
        shot("p5_03_kept_mine")
        ready("SAVED_AFTER_CONFLICT")
    }

    /** Phase 6: the PC revokes this device (the host does it); the app stops and says so. */
    @Test
    fun p6_revoked() {
        waitForLabel("Connected")
        ready("READY_FOR_REVOKE")
        waitForLabel("Not trusted", 90_000)
        shot("p6_01_revoked")
        Thread.sleep(5_000)
        assertEquals("a revoked device kept trying", "Not trusted", label())
    }

    /** Phase 7: a used QR code pairs nothing; a new one pairs again; Forget deletes the pairing and the key. */
    @Test
    fun p7_repair_then_forget() {
        rule.waitUntil("the app settles", 60_000) { label() in setOf("Not trusted", "Connected", "Offline") }
        assertEquals("the revoked pairing came back by itself", "Not trusted", label())
        pairWith(arg("oldlink"))
        rule.onNodeWithTag("pair_button").performClick()
        waitFor("pair_problem")
        Log.i(TAG, "old code: " + textOf("pair_problem"))
        assertTrue(textOf("pair_problem"), textOf("pair_problem").contains("already used"))
        shot("p7_01_old_code_refused")
        rule.onNodeWithTag("pair_link").performTextClearance()
        rule.onNodeWithTag("pair_link").performTextInput(arg("link"))
        waitFor("pair_preview")
        rule.onNodeWithTag("pair_button").performClick()
        waitForLabel("Connected")
        assertEquals("the old key was kept", 1, deviceKeys().size)
        shot("p7_02_paired_again")

        goHome()
        val pcs = instrumentation.targetContext.getSharedPreferences("lcl", 0).getString("pcs", null)
        val id = Regex("\"pc_id\":\"([0-9a-f]+)\"").find(pcs ?: "")?.groupValues?.get(1) ?: error("no paired PC: $pcs")
        rule.onNodeWithTag("forget:$id").performClick()
        rule.onNodeWithTag("forget_confirm").performClick()
        rule.waitUntil("the PC is forgotten", 10_000) { label() == "No PC" }
        shot("p7_03_forgotten")
        assertTrue("a device key survived Forget", deviceKeys().isEmpty())
        val after = instrumentation.targetContext.getSharedPreferences("lcl", 0).getString("pcs", null) ?: "[]"
        assertFalse(after.contains(id))
    }

    private companion object {
        const val TAG = "LclE2E"

        /** Before API 31 this is the only way to ask. */
        @Suppress("DEPRECATION")
        fun legacyLevel(info: KeyInfo) = "insideSecureHardware=${info.isInsideSecureHardware}"
    }
}
