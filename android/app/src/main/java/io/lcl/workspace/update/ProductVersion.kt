package io.lcl.workspace.update

/**
 * The product version as people read it: a release's one trailing ".0"
 * dropped, so 0.5.0 reads "0.5" and 1.2.0 "1.2", while 1.2.3 and every
 * pre-release read in full. Words only: the app's versionName, the update
 * manifest, tags and file names keep the full version, and versionCode is
 * package metadata that is never shown.
 */
object ProductVersion {
    private val release = Regex("[0-9]+\\.[0-9]+\\.0")

    fun shown(version: String): String = if (release.matches(version)) version.dropLast(2) else version

    /** The PC service About names, "lcl-remote 0.5.0", with its version read the same way. */
    fun shownService(service: String): String {
        val name = service.substringBeforeLast(' ', "")
        return if (name.isEmpty()) service else "$name ${shown(service.substringAfterLast(' '))}"
    }
}
