package io.lcl.workspace

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import android.graphics.Bitmap
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
import com.google.zxing.client.android.Intents
import com.journeyapps.barcodescanner.CaptureActivity
import io.lcl.workspace.remote.KeyProtection
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
 * device, approve or deny a pairing request. Arguments come as `-e name
 * value`: `link` (pairing text), `fp`, `workdir`, `oldlink`.
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

    /**
     * Press Pair. The PC records a request and the screen shows this device's
     * verification code; nothing is saved or connected until the person at
     * the PC approves it. The host script is told the code (after [marker]),
     * checks that the PC lists that very code and trusts nothing new yet, and
     * approves or denies the request with `lcl-remote` itself: nothing in this
     * test decides for the PC.
     */
    private fun pressPairAndWaitForThePc(marker: String) {
        val pcsBefore = pairedPcs()
        val keysBefore = deviceKeys().size
        rule.onNodeWithTag("pair_button").performScrollTo().performClick()
        waitFor("pair_verification")
        val code = textOf("pair_verification")
        assertTrue("not a verification code: $code", Regex("[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}").matches(code))
        assertEquals("a pending request saved a PC", pcsBefore, pairedPcs())
        assertEquals("the pending request has no key of its own", keysBefore + 1, deviceKeys().size)
        shot("${marker.lowercase()}_waiting_for_the_pc")
        ready("$marker $code")
    }

    /**
     * Press the app's own Scan QR code button and have the scanner read
     * `link`. The camera activity is never started: the instrumentation
     * answers its launch with the result a scan of the PC's QR code gives, so
     * everything after the camera — the app's launcher, the scan contract and
     * what the app does with the result — is the app's own code.
     */
    private fun scan(link: String) {
        val read = Instrumentation.ActivityResult(Activity.RESULT_OK, Intent().putExtra(Intents.Scan.RESULT, link))
        val camera = instrumentation.addMonitor(CaptureActivity::class.java.name, read, true)
        try {
            rule.onNodeWithTag("scan").performClick()
            rule.waitUntil("the scanner is started and answered", 10_000) { camera.hits > 0 }
        } finally {
            instrumentation.removeMonitor(camera)
        }
    }

    private fun pairedPcs(): String? = instrumentation.targetContext.getSharedPreferences("lcl", 0).getString("pcs", null)

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

    /** Pair from pairing text and stop, once the PC approves (the host does it). */
    @Test
    fun p0_pair_only() {
        waitFor("pair_new")
        pairWith(arg("link"))
        shot("p0_01_pair_confirm")
        pressPairAndWaitForThePc("PENDING_P0")
        waitForLabel("Connected", 120_000)
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
        val labelBefore = label()
        val pcsBefore = pairedPcs()
        // The pairing text the PC printed, pasted on the Pair screen. (No other
        // app can hand it to the app; see IncomingIntentsTest.) It only fills
        // the form in: nothing is trusted, recorded or connected until Pair is
        // pressed — and then not until the PC approves.
        pairWith(link)
        Thread.sleep(2_000)
        val shown = "Fingerprint " + fingerprint.chunked(4).take(8).joinToString(" ")
        rule.onNode(hasText(shown, substring = true), useUnmergedTree = true).assertExists()
        assertTrue("a pasted link paired without the person", deviceKeys().isEmpty())
        assertEquals("a pasted link recorded a PC before Pair", pcsBefore, pairedPcs())
        assertEquals("a pasted link connected before Pair", labelBefore, label())
        shot("p1_02_pair_confirm")
        pressPairAndWaitForThePc("PENDING_P1")
        waitForLabel("Connected", 120_000)

        // The device key: in Android Keystore, one per PC, never exportable.
        val keys = deviceKeys()
        assertEquals(1, keys.size)
        val keystore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val key = keystore.getKey(keys.single(), null) as PrivateKey
        assertNull("the device's private key is exportable", key.encoded)
        val info = KeyFactory.getInstance(key.algorithm, "AndroidKeyStore").getKeySpec(key, KeyInfo::class.java)
        val level = if (Build.VERSION.SDK_INT >= 31) "securityLevel=${info.securityLevel}" else legacyLevel(info)
        Log.i(TAG, "device key ${keys.single()}: ${key.algorithm}, $level")
        // The app reports what the platform says about this very key; it does
        // not assume hardware.
        val container = (instrumentation.targetContext.applicationContext as LclApplication).container
        val reported = container.identities.protection(keys.single())
        val direct = if (Build.VERSION.SDK_INT >= 31) KeyProtection.of(info.securityLevel, false) else KeyProtection.of(null, legacyInside(info))
        assertEquals(direct, reported)
        Log.i(TAG, "device key protection reported by the app: $reported")
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
        assertTrue(textOf("about:This device's key"), textOf("about:This device's key").startsWith("Android Keystore, not exportable · "))
        Log.i(TAG, "about key: ${textOf("about:This device's key")}")
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

    /** Phase 7: a used QR code pairs nothing; a new one pairs again once the PC approves; Forget deletes the pairing and the key. */
    @Test
    fun p7_repair_then_forget() {
        rule.waitUntil("the app settles", 60_000) { label() in setOf("Not trusted", "Connected", "Offline") }
        assertEquals("the revoked pairing came back by itself", "Not trusted", label())
        pairWith(arg("oldlink"))
        rule.onNodeWithTag("pair_button").performScrollTo().performClick()
        waitFor("pair_problem")
        Log.i(TAG, "old code: " + textOf("pair_problem"))
        assertTrue(textOf("pair_problem"), textOf("pair_problem").contains("already used"))
        shot("p7_01_old_code_refused")
        rule.onNodeWithTag("pair_link").performTextClearance()
        rule.onNodeWithTag("pair_link").performTextInput(arg("link"))
        waitFor("pair_preview")
        pressPairAndWaitForThePc("PENDING_P7")
        waitForLabel("Connected", 120_000)
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

    /**
     * Phase 8: the app's own QR scanner. A scanned code only fills the form in,
     * exactly as pasted text does: no key is made, no PC is recorded and
     * nothing connects until Pair is pressed — and then the PC is asked, and
     * it pairs once the PC approves, which also proves the one-time code was
     * still unused. (No camera is used; see [scan].)
     */
    @Test
    fun p8_scan_fills_the_form_and_pairs_only_on_confirmation() {
        val link = arg("link")
        rule.waitUntil("the app settles", 60_000) { label() in setOf("No PC", "Connected", "Offline", "Not trusted") }
        val labelBefore = label()
        goHome()
        rule.onNodeWithTag("pair_new").performClick()
        val keysBefore = deviceKeys()
        val pcsBefore = pairedPcs()

        scan(link)
        waitFor("pair_preview")
        val field = rule.onNodeWithTag("pair_link").fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text
        assertEquals("the scanned link did not fill the form", link, field)
        val shown = "Fingerprint " + PairingLink.parse(link).fingerprint.chunked(4).take(8).joinToString(" ")
        rule.onNode(hasText(shown, substring = true), useUnmergedTree = true).assertExists()
        Thread.sleep(3_000) // time enough for a pairing that must not happen
        assertEquals("a scanned code made a pairing key before Pair", keysBefore, deviceKeys())
        assertEquals("a scanned code recorded a PC before Pair", pcsBefore, pairedPcs())
        assertEquals("a scanned code connected before Pair", labelBefore, label())
        assertFalse("a scanned code was tried before Pair", exists("pair_problem"))
        shot("p8_01_scanned_confirm")

        pressPairAndWaitForThePc("PENDING_P8")
        waitForLabel("Connected", 120_000)
        assertEquals(keysBefore.size + 1, deviceKeys().size)
        shot("p8_02_paired")
    }

    /**
     * Phase 9: pairing text from before approval existed is refused before
     * anything is made; a request the PC denies (the host does it) leaves no
     * key and no record, and the pairing already working stays connected.
     */
    @Test
    fun p9_older_or_denied_pairing_leaves_nothing_and_keeps_the_pairing() {
        waitForLabel("Connected")
        val keysBefore = deviceKeys()
        val pcsBefore = pairedPcs()
        goHome()
        rule.onNodeWithTag("pair_new").performClick()
        val older = arg("link").replace("LCLPAIR|", "lclpair://pair?").replace("v=2", "v=1")
        rule.onNodeWithTag("pair_link").performTextInput(older)
        rule.onNodeWithTag("pair_button").performScrollTo().performClick()
        waitFor("pair_problem")
        Log.i(TAG, "older pairing text: " + textOf("pair_problem"))
        assertTrue(textOf("pair_problem"), textOf("pair_problem").contains("older pairing flow"))
        assertEquals("older pairing text made a key", keysBefore, deviceKeys())
        shot("p9_01_older_text_refused")

        rule.onNodeWithTag("pair_link").performTextClearance()
        rule.onNodeWithTag("pair_link").performTextInput(arg("link"))
        waitFor("pair_preview")
        pressPairAndWaitForThePc("PENDING_P9")
        rule.waitUntil("the PC's denial reaches the phone", 120_000) { exists("pair_problem") }
        Log.i(TAG, "denied: " + textOf("pair_problem"))
        assertTrue(textOf("pair_problem"), textOf("pair_problem").contains("denied"))
        shot("p9_02_denied")
        assertEquals("a denied request kept its key", keysBefore, deviceKeys())
        assertEquals("a denied request recorded a PC", pcsBefore, pairedPcs())
        assertEquals("a denied request broke the working pairing", "Connected", label())
    }

    private companion object {
        const val TAG = "LclE2E"

        /** Before API 31 this is the only way to ask. */
        @Suppress("DEPRECATION")
        fun legacyLevel(info: KeyInfo) = "insideSecureHardware=${info.isInsideSecureHardware}"

        @Suppress("DEPRECATION")
        fun legacyInside(info: KeyInfo) = info.isInsideSecureHardware
    }
}
