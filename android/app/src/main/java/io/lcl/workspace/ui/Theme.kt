package io.lcl.workspace.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.lcl.workspace.data.Theme

/** The desktop workspace's palette, so LCL looks like LCL on both screens. */
@Immutable
data class LclColors(
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
)

private val LightLcl = LclColors(
    keyword = Color(0xFF1F6FB2), block = Color(0xFF5546A8), type = Color(0xFF0F6F5C),
    literal = Color(0xFF8A5D00), string = Color(0xFF8A4B1F), symbol = Color(0xFF5C6472),
    ref = Color(0xFF9B2F70), bad = Color(0xFFB3312C), warn = Color(0xFF8A5D00),
    info = Color(0xFF5546A8), good = Color(0xFF1F7A4D), gutter = Color(0xFFECEEF2), gutterText = Color(0xFF8B93A3),
)

private val DarkLcl = LclColors(
    keyword = Color(0xFF7CB7EA), block = Color(0xFF9D8FF0), type = Color(0xFF56BFA5),
    literal = Color(0xFFD9A441), string = Color(0xFFC99A6E), symbol = Color(0xFF8B93A3),
    ref = Color(0xFFE0A0C8), bad = Color(0xFFE0645F), warn = Color(0xFFD9A441),
    info = Color(0xFF8A7FD4), good = Color(0xFF58C18A), gutter = Color(0xFF101216), gutterText = Color(0xFF5D6575),
)

val LocalLclColors = staticCompositionLocalOf { LightLcl }

@Composable
fun LclTheme(theme: Theme, content: @Composable () -> Unit) {
    val dark = when (theme) {
        Theme.SYSTEM -> isSystemInDarkTheme()
        Theme.DARK -> true
        Theme.LIGHT -> false
    }
    val scheme = if (dark) {
        // The desktop workspace's graphite, with its one blue accent.
        darkColorScheme(
            primary = Color(0xFF63B0EE), onPrimary = Color(0xFF0B1117),
            background = Color(0xFF14171C), surface = Color(0xFF14171C),
            surfaceVariant = Color(0xFF1B1F26), onSurface = Color(0xFFDDE2EA), onBackground = Color(0xFFDDE2EA),
            onSurfaceVariant = Color(0xFF97A0B0), outline = Color(0xFF2A2F39), outlineVariant = Color(0xFF20242C),
            error = Color(0xFFE46A64),
        )
    } else {
        lightColorScheme(
            primary = Color(0xFF1D6FB8), onPrimary = Color.White,
            background = Color(0xFFF4F6F9), surface = Color(0xFFF4F6F9),
            surfaceVariant = Color.White, onSurface = Color(0xFF1F242C), onBackground = Color(0xFF1F242C),
            onSurfaceVariant = Color(0xFF5B6370), outline = Color(0xFFD5DAE2), outlineVariant = Color(0xFFE5E8EE),
            error = Color(0xFFB3312C),
        )
    }
    // Corners stay small: a workspace tool, not a card deck.
    val shapes = Shapes(
        extraSmall = RoundedCornerShape(4.dp), small = RoundedCornerShape(6.dp), medium = RoundedCornerShape(8.dp),
        large = RoundedCornerShape(10.dp), extraLarge = RoundedCornerShape(12.dp),
    )
    androidx.compose.runtime.CompositionLocalProvider(LocalLclColors provides if (dark) DarkLcl else LightLcl) {
        MaterialTheme(colorScheme = scheme, shapes = shapes, content = content)
    }
}
