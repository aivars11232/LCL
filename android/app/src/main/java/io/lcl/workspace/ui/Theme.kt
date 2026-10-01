package io.lcl.workspace.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.lcl.workspace.data.Theme

/**
 * The desktop workspace's palette (its stylesheet's tokens), value for value,
 * so LCL looks like LCL on both screens: the page and its ink, what is raised
 * on the page (buttons, rows, dialogs) and what is sunk into it (strips, the
 * gutter, the files panel), the hairlines, the one accent, the states, and the
 * syntax colours.
 */
@Immutable
data class LclColors(
    val page: Color,
    val ink: Color,
    val accent: Color,
    val onAccent: Color,
    val keyword: Color,
    val block: Color,
    val type: Color,
    val literal: Color,
    val string: Color,
    val symbol: Color,
    val ref: Color,
    val bad: Color,
    val warn: Color,
    val info: Color,
    val good: Color,
    val gutter: Color,
    val gutterText: Color,
    val raised: Color,
    val sunken: Color,
    val hover: Color,
    val line: Color,
    val lineSoft: Color,
    val inkDim: Color,
    val inkFaint: Color,
    /** The accent at the stylesheet's low opacity, over the page: the open file's row, a chosen chip. */
    val accentSoft: Color,
)

private val LightLcl = LclColors(
    page = Color(0xFFF4F6F9), ink = Color(0xFF1F242C), accent = Color(0xFF1D6FB8), onAccent = Color.White,
    keyword = Color(0xFF1D6FB8), block = Color(0xFF5546A8), type = Color(0xFF0F6F5C),
    literal = Color(0xFF8A5D00), string = Color(0xFF8A4B1F), symbol = Color(0xFF5B6370), ref = Color(0xFF9B2F70),
    bad = Color(0xFFB3312C), warn = Color(0xFF8A5D00), info = Color(0xFF5546A8), good = Color(0xFF1F7A4D),
    gutter = Color(0xFFEAEDF2), gutterText = Color(0xFF8A92A1),
    raised = Color.White, sunken = Color(0xFFEAEDF2), hover = Color(0xFFE3E7EE),
    line = Color(0xFFD5DAE2), lineSoft = Color(0xFFE5E8EE),
    inkDim = Color(0xFF5B6370), inkFaint = Color(0xFF8A92A1), accentSoft = Color(0xFFDCE7F2),
)

private val DarkLcl = LclColors(
    page = Color(0xFF14171C), ink = Color(0xFFDDE2EA), accent = Color(0xFF63B0EE), onAccent = Color(0xFF0B1117),
    keyword = Color(0xFF7FBBEE), block = Color(0xFFA496F2), type = Color(0xFF5BC3A9),
    literal = Color(0xFFDFAB48), string = Color(0xFFCF9F72), symbol = Color(0xFF97A0B0), ref = Color(0xFFE4A6CC),
    bad = Color(0xFFE46A64), warn = Color(0xFFDFAB48), info = Color(0xFF948ADE), good = Color(0xFF5CC48F),
    gutter = Color(0xFF0F1216), gutterText = Color(0xFF636C7C),
    raised = Color(0xFF1B1F26), sunken = Color(0xFF0F1216), hover = Color(0xFF232830),
    line = Color(0xFF2A2F39), lineSoft = Color(0xFF20242C),
    inkDim = Color(0xFF97A0B0), inkFaint = Color(0xFF636C7C), accentSoft = Color(0xFF1F2C39),
)

val LocalLclColors = staticCompositionLocalOf { LightLcl }

/**
 * Every role Material's components draw with, from the palette: nothing is
 * left to Material's own defaults, whose lavender is not LCL's. Dialogs and
 * menus are raised, the files drawer is sunk, a chosen item is the accent's
 * soft tint, and surfaces are never tinted by elevation.
 */
private fun scheme(c: LclColors, dark: Boolean): ColorScheme =
    (if (dark) darkColorScheme() else lightColorScheme()).copy(
        primary = c.accent, onPrimary = c.onAccent, primaryContainer = c.accentSoft, onPrimaryContainer = c.accent,
        inversePrimary = if (dark) LightLcl.accent else DarkLcl.accent,
        secondary = c.inkDim, onSecondary = c.page, secondaryContainer = c.accentSoft, onSecondaryContainer = c.accent,
        tertiary = c.info, onTertiary = c.page,
        tertiaryContainer = if (dark) Color(0xFF26233A) else Color(0xFFE4E0F5), onTertiaryContainer = c.info,
        background = c.page, onBackground = c.ink, surface = c.page, onSurface = c.ink,
        surfaceVariant = c.hover, onSurfaceVariant = c.inkDim, surfaceTint = c.page,
        inverseSurface = c.ink, inverseOnSurface = c.page,
        error = c.bad, onError = c.onAccent,
        errorContainer = if (dark) Color(0xFF3A2022) else Color(0xFFF6DEDD), onErrorContainer = c.bad,
        outline = if (dark) Color(0xFF394050) else Color(0xFFBAC2CE), outlineVariant = c.line, scrim = Color.Black,
        surfaceBright = c.hover, surfaceDim = c.sunken,
        surfaceContainerLowest = if (dark) c.sunken else c.raised, surfaceContainerLow = c.sunken,
        surfaceContainer = c.raised, surfaceContainerHigh = c.raised, surfaceContainerHighest = c.hover,
    )

/** Corners stay small, as on the desktop: a workspace tool, not a card deck. */
private val LclShapes = Shapes(
    extraSmall = RoundedCornerShape(6.dp), small = RoundedCornerShape(6.dp), medium = RoundedCornerShape(8.dp),
    large = RoundedCornerShape(10.dp), extraLarge = RoundedCornerShape(12.dp),
)

/** A tighter scale than Material's, for a tool that shows text and paths: no wide tracking, smaller body. */
private val LclType = Typography().run {
    copy(
        headlineMedium = headlineMedium.copy(fontSize = 24.sp, lineHeight = 30.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.2).sp),
        headlineSmall = headlineSmall.copy(fontSize = 20.sp, lineHeight = 26.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.2).sp),
        titleLarge = titleLarge.copy(fontSize = 18.sp, lineHeight = 24.sp, fontWeight = FontWeight.SemiBold),
        titleMedium = titleMedium.copy(fontSize = 15.sp, lineHeight = 21.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.sp),
        bodyLarge = bodyLarge.copy(fontSize = 15.sp, lineHeight = 21.sp, letterSpacing = 0.sp),
        bodyMedium = bodyMedium.copy(fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp),
        bodySmall = bodySmall.copy(fontSize = 12.5.sp, lineHeight = 17.sp, letterSpacing = 0.sp),
        labelLarge = labelLarge.copy(fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium, letterSpacing = 0.sp),
        labelMedium = labelMedium.copy(fontSize = 12.sp, lineHeight = 16.sp, letterSpacing = 0.2.sp),
        labelSmall = labelSmall.copy(fontSize = 11.sp, lineHeight = 15.sp, letterSpacing = 0.6.sp),
    )
}

@Composable
fun LclTheme(theme: Theme, content: @Composable () -> Unit) {
    val dark = when (theme) {
        Theme.SYSTEM -> isSystemInDarkTheme()
        Theme.DARK -> true
        Theme.LIGHT -> false
    }
    val colors = if (dark) DarkLcl else LightLcl
    CompositionLocalProvider(LocalLclColors provides colors) {
        MaterialTheme(colorScheme = scheme(colors, dark), shapes = LclShapes, typography = LclType, content = content)
    }
}
