package io.lcl.workspace.editor

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.lcl.workspace.ui.LclColors
import io.lcl.workspace.ui.LocalLclColors
import io.lcl.workspace.workspace.OpenDocument

/** Four spaces: LCL indents with spaces, and a tab character is an error in LCL source. */
const val INDENT = "    "

/** Selection and history for one open document, kept while the workspace is on screen. */
class EditorState {
    var selection by mutableStateOf(TextRange(0))
    val history = EditHistory()

    fun snapshot(text: String) = EditHistory.Snapshot(text, selection.start, selection.end)

    /** Undo: returns the text to show, having moved the cursor back where it was. */
    fun undo(text: String): String? = history.undo(snapshot(text))?.also { selection = TextRange(it.selectionStart, it.selectionEnd) }?.text
    fun redo(text: String): String? = history.redo(snapshot(text))?.also { selection = TextRange(it.selectionStart, it.selectionEnd) }?.text

    /** Replace the selection with `inserted`, for the indent key and button. */
    fun insert(text: String, inserted: String): String {
        val start = minOf(selection.start, selection.end).coerceIn(0, text.length)
        val end = maxOf(selection.start, selection.end).coerceIn(0, text.length)
        history.record(snapshot(text), System.currentTimeMillis())
        selection = TextRange(start + inserted.length)
        return text.substring(0, start) + inserted + text.substring(end)
    }
}

/**
 * The source editor.
 *
 * What is drawn is always the document's own text. Highlighting is a visual
 * transformation that only adds colour to it, from token spans the PC's lexer
 * produced and diagnostic spans its engine reported, both moved with the text
 * as it is edited; before the PC has answered, the text is simply plain.
 * Nothing about highlighting can hide a character.
 *
 * The line-number gutter and the text share one vertical scroll and one line
 * height, so they cannot drift apart, and lines never wrap: a long line
 * scrolls sideways instead, so line N is always row N.
 */
@Composable
fun CodeEditor(
    document: OpenDocument,
    state: EditorState,
    onTextChange: (String) -> Unit,
    readOnly: Boolean,
    fontSize: Int,
    lineNumbers: Boolean,
    modifier: Modifier = Modifier,
) {
    val colors = LocalLclColors.current
    val ink = MaterialTheme.colorScheme.onSurface
    val style = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontSize = fontSize.sp,
        lineHeight = (fontSize * 1.5).sp,
        lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
        color = ink,
    )
    val text = document.text
    val selection = TextRange(state.selection.start.coerceIn(0, text.length), state.selection.end.coerceIn(0, text.length))
    val highlight = remember(text, document.tokens, document.marks, colors) { Highlight(document, colors) }
    val vertical = rememberScrollState()
    val horizontal = rememberScrollState()

    Row(modifier.verticalScroll(vertical)) {
        if (lineNumbers) {
            Text(
                text = gutter(text, document, colors),
                style = style.copy(color = colors.gutterText, textAlign = TextAlign.End),
                modifier = Modifier
                    .background(colors.gutter)
                    .padding(horizontal = 8.dp, vertical = 8.dp)
                    .testTag("gutter"),
            )
        }
        BoxWithConstraints(Modifier.weight(1f)) {
            val visible = maxWidth
            Box(Modifier.horizontalScroll(horizontal)) {
                BasicTextField(
                    value = TextFieldValue(text, selection),
                    onValueChange = { value ->
                        if (value.text != text) {
                            state.history.record(EditHistory.Snapshot(text, selection.start, selection.end), System.currentTimeMillis())
                            state.selection = value.selection
                            onTextChange(value.text)
                        } else {
                            state.selection = value.selection
                        }
                    },
                    readOnly = readOnly,
                    textStyle = style,
                    cursorBrush = SolidColor(ink),
                    visualTransformation = highlight,
                    modifier = Modifier
                        .widthIn(min = visible)
                        .padding(8.dp)
                        .testTag("source")
                        .onPreviewKeyEvent { event ->
                            if (!readOnly && event.key == Key.Tab && event.type == KeyEventType.KeyDown) {
                                onTextChange(state.insert(text, INDENT))
                                true
                            } else {
                                false
                            }
                        },
                )
            }
        }
    }
}

/** Line numbers from the text itself, with the lines holding a diagnostic marked. */
private fun gutter(text: String, document: OpenDocument, colors: LclColors): AnnotatedString {
    val count = lineCount(text)
    val marked = document.marks.mapNotNull { m -> m.line?.let { it to m.kind } }.groupBy({ it.first }, { it.second })
    return AnnotatedString.Builder().apply {
        for (line in 1..count) {
            val kinds = marked[line]
            if (kinds != null) {
                pushStyle(SpanStyle(color = if ("bad" in kinds) colors.bad else colors.warn, fontWeight = FontWeight.Bold))
                append(line.toString())
                pop()
            } else {
                append(line.toString())
            }
            if (line < count) append('\n')
        }
    }.toAnnotatedString()
}

/** Colour from the PC's spans. The text itself is never changed, only styled. */
class Highlight(private val document: OpenDocument, private val colors: LclColors) : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        if (text.text != document.text) return TransformedText(text, OffsetMapping.Identity)
        val index = Utf8Index(document.text)
        val length = document.text.length
        val builder = AnnotatedString.Builder(text)
        fun range(start: Int, end: Int): IntRange? {
            val from = index.charOf(start).coerceIn(0, length)
            val to = index.charOf(end).coerceIn(0, length)
            return if (to > from) from until to else null
        }
        document.tokens?.forEach { span ->
            val color = when (span.kind) {
                "keyword" -> colors.keyword
                "block" -> colors.block
                "type" -> colors.type
                "literal" -> colors.literal
                "string" -> colors.string
                "symbol" -> colors.symbol
                else -> null
            } ?: return@forEach
            val r = range(span.start, span.end) ?: return@forEach
            builder.addStyle(
                SpanStyle(color = color, fontWeight = if (span.kind == "block") FontWeight.SemiBold else null),
                r.first,
                r.last + 1,
            )
        }
        document.marks.forEach { mark ->
            val color = when (mark.kind) {
                "bad" -> colors.bad
                "warn" -> colors.warn
                else -> colors.info
            }
            val r = range(mark.start, maxOf(mark.end, mark.start + 1)) ?: return@forEach
            builder.addStyle(
                SpanStyle(background = color.copy(alpha = 0.18f), textDecoration = TextDecoration.Underline),
                r.first,
                r.last + 1,
            )
        }
        return TransformedText(builder.toAnnotatedString(), OffsetMapping.Identity)
    }
}
