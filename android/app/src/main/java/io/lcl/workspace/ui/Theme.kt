package io.lcl.workspace.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
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
        darkColorScheme(
            primary = Color(0xFF5AA9E6), onPrimary = Color(0xFF0C1116),
            background = Color(0xFF16181D), surface = Color(0xFF16181D),
            surfaceVariant = Color(0xFF1C1F26), onSurface = Color(0xFFD7DCE5), onBackground = Color(0xFFD7DCE5),
            error = Color(0xFFE0645F),
        )
    } else {
        lightColorScheme(
            primary = Color(0xFF1F6FB2), onPrimary = Color.White,
            background = Color(0xFFF6F7F9), surface = Color(0xFFF6F7F9),
            surfaceVariant = Color.White, onSurface = Color(0xFF232830), onBackground = Color(0xFF232830),
            error = Color(0xFFB3312C),
        )
    }
    androidx.compose.runtime.CompositionLocalProvider(LocalLclColors provides if (dark) DarkLcl else LightLcl) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
