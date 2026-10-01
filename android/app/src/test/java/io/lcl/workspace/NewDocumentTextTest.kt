package io.lcl.workspace

import io.lcl.workspace.workspace.NEW_DOCUMENT_TEXT
import org.junit.Assert.assertEquals
import org.junit.Test

/** The starting text of a new document is exact: every byte of it reaches the file. */
class NewDocumentTextTest {
    @Test
    fun a_new_document_starts_with_exactly_this_text() {
        assertEquals(
            "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.new\n" +
                "    NAME: \"New document\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n" +
                "    DOMAIN: \"general\"\n",
            NEW_DOCUMENT_TEXT,
        )
    }
}
