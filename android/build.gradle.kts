// LCL for Android. The app is a native client of the PC's LCL: it holds no
// LCL engine, parser or runtime of its own. See README.md.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
