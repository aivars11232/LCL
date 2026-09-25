package io.lcl.workspace

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/**
 * What the app's manifest lets other apps hand it. A pairing link carries a
 * one-time code that pairs whoever uses it first, and any app can register a
 * custom scheme, so no filter takes `lclpair://` links and none is BROWSABLE:
 * a camera app, a browser or a message cannot pass one to this app. The
 * launcher and `.lcl` / `.lcl.txt` documents still open it. The instrumented
 * IncomingIntentsTest checks the same against the installed app.
 */
class ManifestTest {
    private val android = "http://schemas.android.com/apk/res/android"

    private val manifest: Element by lazy {
        val file = listOf("src/main/AndroidManifest.xml", "app/src/main/AndroidManifest.xml").map(::File).first { it.isFile }
        DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
            .newDocumentBuilder().parse(file).documentElement
    }

    private fun Element.all(tag: String): List<Element> =
        getElementsByTagName(tag).let { nodes -> List(nodes.length) { nodes.item(it) as Element } }

    private fun Element.values(tag: String, attribute: String): Set<String> =
        all(tag).mapNotNull { it.getAttributeNS(android, attribute).ifEmpty { null } }.toSet()

    @Test
    fun no_other_app_can_hand_the_app_a_pairing_link() {
        val filters = manifest.all("intent-filter")
        assertTrue(filters.isNotEmpty())
        for (filter in filters) {
            val schemes = filter.values("data", "scheme")
            assertFalse("a filter takes pairing links: $schemes", "lclpair" in schemes)
            val categories = filter.values("category", "name")
            assertFalse("a filter can be opened from a browser or a link: $schemes", "android.intent.category.BROWSABLE" in categories)
        }
    }

    @Test
    fun the_launcher_and_lcl_documents_still_open_the_app() {
        val main = manifest.all("activity").single { it.getAttributeNS(android, "name") == ".MainActivity" }
        val filters = main.all("intent-filter")
        assertTrue(
            "the launcher entry is gone",
            filters.any {
                it.values("action", "name") == setOf("android.intent.action.MAIN") &&
                    it.values("category", "name") == setOf("android.intent.category.LAUNCHER")
            },
        )
        val documents = filters.single { it.values("data", "pathPattern").isNotEmpty() }
        assertEquals(setOf("android.intent.action.VIEW"), documents.values("action", "name"))
        assertEquals(setOf("android.intent.category.DEFAULT"), documents.values("category", "name"))
        assertEquals(setOf("file", "content"), documents.values("data", "scheme"))
        // Exactly .lcl and .lcl.txt, as the manifest writes them; no plain .txt.
        assertEquals(
            setOf(""".*\\.lcl""", """.*\\..*\\.lcl""", """.*\\.lcl\\.txt""", """.*\\..*\\.lcl\\.txt"""),
            documents.values("data", "pathPattern"),
        )
    }
}
