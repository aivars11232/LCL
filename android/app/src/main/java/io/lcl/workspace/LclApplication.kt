package io.lcl.workspace

import android.app.Application
import android.content.Context
import io.lcl.workspace.connection.AndroidNetwork
import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.connection.PairingController
import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.data.PcStore
import io.lcl.workspace.data.PreferencesStore
import io.lcl.workspace.data.SettingsStore
import io.lcl.workspace.local.LocalProjects
import io.lcl.workspace.remote.Discovery
import io.lcl.workspace.remote.KeystoreIdentities
import io.lcl.workspace.remote.TlsTransport
import io.lcl.workspace.update.AndroidPackageFacts
import io.lcl.workspace.update.GitHubReleases
import io.lcl.workspace.update.PackageInstallerApk
import io.lcl.workspace.update.TrustedKey
import io.lcl.workspace.update.UpdateController
import io.lcl.workspace.workspace.WorkspaceController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.withContext

class LclApplication : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        // Pairing outlives the process: whenever the app starts, it goes back
        // to the PC it was working with, without a QR code.
        container.connection.start()
        // An update check needs no PC, and runs only when a day has passed.
        container.updates.checkWhenDue()
    }
}

/** The app's singletons, alive as long as the process is. */
class AppContainer(context: Context) {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val store = PreferencesStore(context.getSharedPreferences("lcl", Context.MODE_PRIVATE))
    val identities = KeystoreIdentities()
    val pcs = PcStore(store)
    private val settingsStore = SettingsStore(store)
    private val _settings = MutableStateFlow(settingsStore.load())
    val settings: StateFlow<AppSettings> = _settings

    val connection = ConnectionManager(
        store = pcs,
        identities = identities,
        transport = TlsTransport(),
        network = AndroidNetwork(context),
        scope = scope,
        discover = { pcId -> withContext(Dispatchers.IO) { Discovery.find(pcId) } },
    )
    /** Projects on this phone: app-private files, never cache. */
    val localProjects = LocalProjects(java.io.File(context.filesDir, "local-projects"))
    val workspace = WorkspaceController(connection, scope, local = localProjects)
    /** The pairing attempt, which outlives the Pair screen. */
    val pairing = PairingController(connection, scope)

    /** Update System V1: the official releases, checked with the keys this build trusts. */
    val updates = UpdateController(
        store = store,
        source = GitHubReleases(BuildConfig.UPDATE_TEST_ENDPOINT),
        keys = { TrustedKey.parse(BuildConfig.UPDATE_TRUSTED_KEYS + "\n" + BuildConfig.UPDATE_TEST_KEYS) },
        facts = AndroidPackageFacts(context),
        installer = PackageInstallerApk(context),
        cacheDir = context.cacheDir,
        scope = scope,
    )

    fun updateSettings(settings: AppSettings) {
        settingsStore.save(settings)
        _settings.value = settingsStore.load()
    }
}
