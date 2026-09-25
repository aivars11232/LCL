package io.lcl.workspace

import android.content.ActivityNotFoundException
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.SystemClock
import android.provider.MediaStore
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore
import java.util.Base64

/**
 * What other apps can hand LCL, checked against the installed app. A pairing
 * link carries a one-time code, and any app can register a custom scheme. The
 * code alone trusts no device — holding it only lets a device make a pending
 * request, and the PC must approve that request's certificate — but pairing
 * starts only in LCL itself, so no app — a camera app, a browser, a message —
 * can pass a link to LCL: no activity takes such links, and one sent to the
 * app by name fills nothing in. Only the app's own scanner and the Pair
 * screen's text field fill the form. Documents (.lcl, .lcl.txt) still open
 * from other apps. No PC is needed.
 */
@RunWith(AndroidJUnit4::class)
class IncomingIntentsTest {
    @get:Rule
    val rule = createAndroidComposeRule<MainActivity>()

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    /** A well-formed pairing link, for a PC that is not there. */
    private val link = "lclpair://pair?v=1&pc=0123456789abcdef0123456789abcdef&n=Some%20PC&fp=${"a".repeat(64)}" +
        "&a=10.0.2.2:9&c=${Base64.getUrlEncoder().withoutPadding().encodeToString(ByteArray(32) { 7 })}&e=4102444800"

    /** The same PC's pairing text as the app reads it now. */
    private val text = link.replace("lclpair://pair?", "LCLPAIR|").replace("v=1", "v=2")

    private fun exists(tag: String) = rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun source(): String =
        rule.onNodeWithTag("source").fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text ?: ""

    /** The activities of this app the system would start for `intent`. */
    @Suppress("DEPRECATION") // the flags-as-Int form is the one API 29 has
    private fun handlers(intent: Intent): List<String> =
        context.packageManager.queryIntentActivities(Intent(intent).setPackage(context.packageName), PackageManager.MATCH_DEFAULT_ONLY)
            .map { it.activityInfo.name }

    private fun send(intent: Intent) = context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))

    private fun deviceKeys(): List<String> {
        val keystore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        return keystore.aliases().toList().filter { it.startsWith("lcl-pc-") }
    }

    private fun pairedPcs(): String? = context.getSharedPreferences("lcl", 0).getString("pcs", null)

    @Test
    fun no_activity_of_the_app_takes_a_pairing_link() {
        val view = Intent(Intent.ACTION_VIEW, Uri.parse(link))
        assertEquals(emptyList<String>(), handlers(view))
        assertEquals(emptyList<String>(), handlers(Intent(view).addCategory(Intent.CATEGORY_BROWSABLE)))
        // Sent as a camera app or a browser sends a link: nothing here takes it.
        assertThrows(ActivityNotFoundException::class.java) {
            send(Intent(view).addCategory(Intent.CATEGORY_BROWSABLE).setPackage(context.packageName))
        }
        assertFalse(exists("pair_link"))
        // Nor is the current pairing text, which is no link at all, taken
        // when it is shared or opened as text.
        assertEquals(emptyList<String>(), handlers(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)))
        assertEquals(emptyList<String>(), handlers(Intent(Intent.ACTION_VIEW, Uri.parse(text))))
    }

    @Test
    fun a_pairing_link_sent_to_the_app_by_name_fills_nothing_in() {
        // The launcher activity is exported, so any app can name it; what it
        // is sent is taken for a document or for nothing.
        rule.waitForIdle()
        val keysBefore = deviceKeys()
        val pcsBefore = pairedPcs()
        send(Intent(Intent.ACTION_VIEW, Uri.parse(link)).setClass(context, MainActivity::class.java))
        val until = SystemClock.uptimeMillis() + 3_000
        while (SystemClock.uptimeMillis() < until) {
            assertFalse("a link from another app filled the Pair screen in", exists("pair_link") || exists("pair_preview"))
            Thread.sleep(100)
        }
        assertEquals("a link from another app made a pairing key", keysBefore, deviceKeys())
        assertEquals("a link from another app recorded a PC", pcsBefore, pairedPcs())

        // The same way in delivers a document: the link above did arrive.
        val text = "LCL:\n    NAME: \"sent by name\"\n"
        val file = File(context.cacheDir, "sent.lcl").apply { writeText(text) }
        send(Intent(Intent.ACTION_VIEW, Uri.fromFile(file)).setClass(context, MainActivity::class.java))
        rule.waitUntil("the document is shown", 10_000) { exists("source") }
        assertEquals(text, source())
    }

    @Test
    fun lcl_documents_from_other_apps_still_open_the_app_and_plain_txt_does_not() {
        // How a file manager or another app links to a document.
        val provider = "content://com.android.externalstorage.documents/document/primary%3ADownload%2F"
        val folder = "file:///storage/emulated/0/Download/"
        for ((name, type) in listOf("todo.lcl" to "application/octet-stream", "notes.lcl.txt" to "text/plain")) {
            for (uri in listOf(provider + name, folder + name)) {
                val intent = Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse(uri), type)
                assertEquals(uri, listOf(MainActivity::class.java.name), handlers(intent))
            }
        }
        for (uri in listOf(provider + "notes.txt", folder + "notes.txt")) {
            val intent = Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse(uri), "text/plain")
            assertEquals(uri, emptyList<String>(), handlers(intent))
        }
    }

    /**
     * A document opened as another app opens one: a file in the phone's
     * Download folder, linked by file://, which the system resolves against
     * the app's document filter. It is shown exactly, through the same strict
     * UTF-8 reading as any opened file. (Reading a Download file by its path
     * needs Android 11, API 30.)
     */
    @Test
    fun an_lcl_or_lcl_txt_file_linked_from_another_app_opens_exactly() {
        assumeTrue(Build.VERSION.SDK_INT >= 30)
        val resolver = context.contentResolver
        val stamp = System.currentTimeMillis()
        for ((ending, type) in listOf(".lcl" to "application/octet-stream", ".lcl.txt" to "text/plain")) {
            val name = "lcl-intent-$stamp$ending"
            val text = "LCL:\n    NAME: \"$name café ā € 𝄞\"\n"
            val row = resolver.insert(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, name)
                    put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
                },
            )!!
            try {
                resolver.openOutputStream(row)!!.use { it.write(text.toByteArray()) }
                @Suppress("DEPRECATION") // the path other apps link to
                val file = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), name)
                assertTrue("the Download folder did not keep the name $name", file.isFile)
                send(Intent(Intent.ACTION_VIEW).setDataAndType(Uri.fromFile(file), type).setPackage(context.packageName))
                rule.waitUntil("$name is shown", 10_000) { exists("source") && source() == text }
            } finally {
                resolver.delete(row, null, null)
            }
        }
    }
}
