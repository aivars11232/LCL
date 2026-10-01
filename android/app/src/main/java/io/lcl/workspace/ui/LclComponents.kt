package io.lcl.workspace.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/*
 * The desktop workspace's controls, on the phone. Every screen of the app
 * uses these instead of Material's own of the same names, so LCL looks like
 * LCL on both screens: squared corners, a hairline border on what is raised,
 * one accent, tabs marked by a bar rather than a pill. Sizes keep Material's
 * minimum touch targets.
 */

private val ButtonPadding = PaddingValues(horizontal = 14.dp, vertical = 8.dp)

/** The action that leads: filled with the accent. */
@Composable
fun Button(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentPadding: PaddingValues = ButtonPadding,
    content: @Composable RowScope.() -> Unit,
) =
    androidx.compose.material3.Button(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        shape = MaterialTheme.shapes.small,
        contentPadding = contentPadding,
        content = content,
    )

/** An ordinary action: a raised surface with a hairline border, as the desktop's buttons. */
@Composable
fun OutlinedButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentPadding: PaddingValues = ButtonPadding,
    content: @Composable RowScope.() -> Unit,
) =
    androidx.compose.material3.OutlinedButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        shape = MaterialTheme.shapes.small,
        colors =
            ButtonDefaults.outlinedButtonColors(
                containerColor = LocalLclColors.current.raised,
                contentColor = MaterialTheme.colorScheme.onSurface,
            ),
        border =
            BorderStroke(
                1.dp,
                if (enabled) MaterialTheme.colorScheme.outline else LocalLclColors.current.line,
            ),
        contentPadding = contentPadding,
        content = content,
    )

/** A quiet action: text in the accent, nothing around it. */
@Composable
fun TextButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentPadding: PaddingValues = PaddingValues(horizontal = 10.dp, vertical = 8.dp),
    content: @Composable RowScope.() -> Unit,
) =
    androidx.compose.material3.TextButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        shape = MaterialTheme.shapes.small,
        contentPadding = contentPadding,
        content = content,
    )

/** One choice among a few: squared, and tinted with the accent when chosen. */
@Composable
fun FilterChip(
    selected: Boolean,
    onClick: () -> Unit,
    label: @Composable () -> Unit,
    modifier: Modifier = Modifier,
) =
    androidx.compose.material3.FilterChip(
        selected = selected,
        onClick = onClick,
        label = label,
        modifier = modifier,
        shape = MaterialTheme.shapes.small,
        colors =
            FilterChipDefaults.filterChipColors(
                containerColor = LocalLclColors.current.raised,
                labelColor = MaterialTheme.colorScheme.onSurface,
                selectedContainerColor = LocalLclColors.current.accentSoft,
                selectedLabelColor = MaterialTheme.colorScheme.primary,
            ),
    )

/**
 * On or off, readable in both states: the accent when on; when off, a dim knob on a sunk track with
 * a border.
 */
@Composable
fun Switch(checked: Boolean, onCheckedChange: ((Boolean) -> Unit)?, modifier: Modifier = Modifier) =
    androidx.compose.material3.Switch(
        checked = checked,
        onCheckedChange = onCheckedChange,
        modifier = modifier,
        colors =
            SwitchDefaults.colors(
                checkedThumbColor = LocalLclColors.current.onAccent,
                checkedTrackColor = MaterialTheme.colorScheme.primary,
                uncheckedThumbColor = LocalLclColors.current.inkDim,
                uncheckedTrackColor = LocalLclColors.current.sunken,
                uncheckedBorderColor = MaterialTheme.colorScheme.outline,
            ),
    )

/** A block set apart: raised, with a hairline border and no shadow. */
@Composable
fun Card(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) =
    androidx.compose.material3.Card(
        modifier = modifier,
        shape = MaterialTheme.shapes.medium,
        colors =
            CardDefaults.cardColors(
                containerColor = LocalLclColors.current.raised,
                contentColor = MaterialTheme.colorScheme.onSurface,
            ),
        border = BorderStroke(1.dp, LocalLclColors.current.line),
        content = content,
    )

/**
 * One tab of a strip, as the desktop's: plain text, and a bar of the accent along its edge when it
 * is the one shown — under it, or over it for the bar at the bottom of the screen. Selection is
 * said to accessibility services as a tab's.
 */
@Composable
fun LclTab(
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    barOnTop: Boolean = false,
    content: @Composable RowScope.() -> Unit,
) {
    val accent = MaterialTheme.colorScheme.primary
    Row(
        modifier
            .heightIn(min = 44.dp)
            .selectable(selected = selected, onClick = onClick, role = Role.Tab)
            .drawBehind {
                if (selected) {
                    val bar = 2.dp.toPx()
                    drawRect(
                        accent,
                        topLeft = Offset(0f, if (barOnTop) 0f else size.height - bar),
                        size = Size(size.width, bar),
                    )
                }
            }
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        CompositionLocalProvider(
            LocalContentColor provides
                if (selected) MaterialTheme.colorScheme.onSurface
                else LocalLclColors.current.inkDim,
            LocalTextStyle provides
                MaterialTheme.typography.labelLarge.copy(
                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium
                ),
        ) {
            content()
        }
    }
}

/**
 * A small mark that acts, as the desktop's panel heads use (⌂ ↻ + ▤): dim until pressed, and what
 * it does is said to accessibility services.
 */
@Composable
fun IconAction(
    glyph: String,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) =
    androidx.compose.material3.TextButton(
        onClick = onClick,
        modifier = modifier.width(44.dp).semantics { contentDescription = description },
        enabled = enabled,
        shape = MaterialTheme.shapes.small,
        colors = ButtonDefaults.textButtonColors(contentColor = LocalLclColors.current.inkDim),
        contentPadding = PaddingValues(0.dp),
    ) {
        Text(glyph, fontSize = 19.sp)
    }

/** The small spaced label over a group, as the desktop's panel heads. */
@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) =
    Text(
        text,
        modifier = modifier,
        style = MaterialTheme.typography.labelSmall,
        color = LocalLclColors.current.inkFaint,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 1.2.sp,
    )

/** A row of a list, as the desktop's project rows: raised, with a hairline border. */
@Composable
fun Modifier.listRow(): Modifier =
    this.clip(MaterialTheme.shapes.medium)
        .background(LocalLclColors.current.raised)
        .border(1.dp, LocalLclColors.current.lineSoft, MaterialTheme.shapes.medium)

/** A one-pixel rule along the top or the bottom edge, as the desktop's separators. */
fun Modifier.hairline(color: Color, top: Boolean): Modifier = drawBehind {
    val stroke = 1.dp.toPx()
    val y = if (top) stroke / 2 else size.height - stroke / 2
    drawLine(color, Offset(0f, y), Offset(size.width, y), strokeWidth = stroke)
}
