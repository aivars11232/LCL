package io.lcl.workspace.update

import io.lcl.workspace.data.KeyValueStore
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/** What the phone knows about itself, and what Android says about an APK file. */
interface PackageFacts {
    fun installed(): InstalledApp
    fun archive(file: File): ApkFacts?
}

/** Handing an APK to Android's own installer. */
interface ApkInstaller {
    /** Whether the person allowed LCL to install apps. */
    fun canInstall(): Boolean

    /** Open the system setting where the person allows it. */
    fun askPermission()

    /** Start a PackageInstaller session; the result comes to [UpdateController.onInstallStatus]. */
    fun install(file: File)
}

enum class Phase { Idle, Checking, UpToDate, Available, Downloading, NeedsPermission, Installing, Failed, Offline, NotConfigured }

data class UpdateUi(
    val phase: Phase = Phase.Idle,
    val installed: String = "",
    val manifest: UpdateManifest? = null,
    val progress: Pair<Long, Long>? = null,
    /** (kind, message): offline, invalid, verification, download, install, unsupported, not_configured, cancelled. */
    val problem: Pair<String, String>? = null,
    val lastCheck: Long? = null,
    val waitingForConfirmation: Boolean = false,
)

/**
 * The update flow. Checking needs no connection to a PC. "Update" is one
 * press: the APK is downloaded, checked against the signed manifest and the
 * installed app, and handed to Android, which may ask the person to confirm.
 * Android installs in place, keeping the app's data and pairings, or leaves
 * the installed app exactly as it was.
 */
class UpdateController(
    private val store: KeyValueStore,
    private val source: ReleaseSource,
    private val keys: () -> List<TrustedKey>,
    private val facts: PackageFacts,
    private val installer: ApkInstaller,
    private val cacheDir: File,
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private val _ui = MutableStateFlow(UpdateUi(lastCheck = store.get(LAST_CHECK)?.toLongOrNull()))
    val ui: StateFlow<UpdateUi> = _ui
    private var release: Release? = null
    private var staged: File? = null

    private fun set(change: (UpdateUi) -> UpdateUi) {
        _ui.value = change(_ui.value)
    }

    private fun fail(e: Throwable) = set {
        when (e) {
            is Offline -> it.copy(phase = Phase.Offline, progress = null, problem = "offline" to (e.message ?: "offline"))
            is UpdateRefused -> it.copy(
                phase = if (e.kind == "not_configured") Phase.NotConfigured else Phase.Failed,
                progress = null,
                problem = e.kind to (e.message ?: e.kind),
            )
            else -> it.copy(phase = Phase.Failed, progress = null, problem = "download" to (e.message ?: e.toString()))
        }
    }

    /** Check when the last successful check is a day old; never in the way. */
    fun checkWhenDue() {
        val last = store.get(LAST_SUCCESS)?.toLongOrNull()
        if (last == null || now() - last >= DAY_MS) check()
    }

    fun check() {
        if (_ui.value.phase in setOf(Phase.Checking, Phase.Downloading, Phase.Installing)) return
        set { it.copy(phase = Phase.Checking, problem = null, installed = runCatching { facts.installed().versionName }.getOrDefault("")) }
        scope.launch {
            try {
                val found = withContext(io) { find() }
                store.put(LAST_SUCCESS, now().toString())
                set { it.copy(phase = if (found == null) Phase.UpToDate else Phase.Available, manifest = found) }
            } catch (e: Throwable) {
                fail(e)
            } finally {
                store.put(LAST_CHECK, now().toString())
                set { it.copy(lastCheck = now()) }
            }
        }
    }

    /** The newer release's verified manifest, or null when the app is current. */
    private fun find(): UpdateManifest? {
        val trusted = keys()
        if (trusted.isEmpty()) throw UpdateRefused("not_configured", NOT_CONFIGURED)
        val latest = source.latest() ?: return null
        for (name in listOf(MANIFEST, SIGNATURE)) {
            if (name !in latest.assets) throw UpdateRefused("invalid", "release ${latest.tag} has no $name")
        }
        val manifest = verifyManifest(source.fetch(latest, MANIFEST, MAX_MANIFEST), source.fetch(latest, SIGNATURE, 1024), trusted)
        if (manifest.releaseTag != latest.tag) throw UpdateRefused("verification", "release ${latest.tag} carries the manifest of ${manifest.releaseTag}")
        if (latest.assets[manifest.android.artifactName] != manifest.android.size) {
            throw UpdateRefused("invalid", "release ${latest.tag} does not hold the APK its manifest names")
        }
        release = latest
        return manifest.takeIf { isNewer(it, facts.installed()) }
    }

    /** Download, verify, and install the update found. */
    fun update() {
        val manifest = _ui.value.manifest ?: return
        val latest = release ?: return
        if (_ui.value.phase !in setOf(Phase.Available, Phase.Failed, Phase.NeedsPermission)) return
        set { it.copy(phase = Phase.Downloading, problem = null, progress = 0L to manifest.android.size) }
        scope.launch {
            try {
                val file = withContext(io) {
                    val bytes = source.fetch(latest, manifest.android.artifactName, manifest.android.size) { done, total ->
                        set { it.copy(progress = done to total) }
                    }
                    val dir = File(cacheDir, "update").apply { deleteRecursively(); mkdirs() }
                    val file = File(dir, manifest.android.artifactName)
                    file.writeBytes(bytes)
                    try {
                        checkApk(bytes, facts.archive(file), manifest, facts.installed())
                    } catch (e: Throwable) {
                        file.delete()
                        throw e
                    }
                    file
                }
                staged = file
                set { it.copy(progress = null) }
                install()
            } catch (e: Throwable) {
                fail(e)
            }
        }
    }

    /** Hand the verified APK to Android, once the person has allowed LCL to install apps. */
    fun install() {
        val file = staged ?: return
        if (!installer.canInstall()) {
            set { it.copy(phase = Phase.NeedsPermission) }
            return
        }
        set { it.copy(phase = Phase.Installing, waitingForConfirmation = false, problem = null) }
        try {
            installer.install(file)
        } catch (e: Throwable) {
            fail(UpdateRefused("install", "Android could not start the installation: ${e.message}"))
        }
    }

    fun askPermission() = installer.askPermission()

    /** Back from the system setting: continue if the person allowed it. */
    fun resume() {
        if (_ui.value.phase == Phase.NeedsPermission && installer.canInstall()) install()
    }

    /**
     * The installer's answer. [confirm] shows Android's own confirmation when
     * it needs the person's approval.
     */
    fun onInstallStatus(status: Int, message: String?, confirm: (() -> Unit)?) {
        when (status) {
            STATUS_PENDING_USER_ACTION -> {
                set { it.copy(phase = Phase.Installing, waitingForConfirmation = true) }
                confirm?.invoke()
            }
            STATUS_SUCCESS -> {
                staged?.delete()
                staged = null
                set { it.copy(phase = Phase.UpToDate, manifest = null, waitingForConfirmation = false) }
            }
            STATUS_FAILURE_ABORTED -> set {
                it.copy(phase = Phase.Failed, waitingForConfirmation = false,
                    problem = "cancelled" to "The installation was cancelled. LCL was not changed; press Update to try again.")
            }
            else -> set {
                it.copy(phase = Phase.Failed, waitingForConfirmation = false,
                    problem = "install" to "Android did not install the update (${message ?: "status $status"}). LCL was not changed.")
            }
        }
    }

    companion object {
        const val MANIFEST = "update-manifest.json"
        const val SIGNATURE = "update-manifest.sig"
        const val DAY_MS = 24L * 60 * 60 * 1000
        const val LAST_CHECK = "update.last_check"
        const val LAST_SUCCESS = "update.last_success"

        // android.content.pm.PackageInstaller's status codes, as its documentation fixes them.
        const val STATUS_PENDING_USER_ACTION = -1
        const val STATUS_SUCCESS = 0
        const val STATUS_FAILURE_ABORTED = 3
    }
}
