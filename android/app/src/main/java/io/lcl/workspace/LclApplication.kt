package io.lcl.workspace

import android.app.Application
import android.content.Context
import io.lcl.workspace.connection.AndroidNetwork
import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.data.AppSettings
import io.lcl.workspace.data.PcStore
import io.lcl.workspace.data.PreferencesStore
import io.lcl.workspace.data.SettingsStore
import io.lcl.workspace.remote.Discovery
import io.lcl.workspace.remote.KeystoreIdentities
import io.lcl.workspace.remote.TlsTransport
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
    val workspace = WorkspaceController(connection, scope)

    fun updateSettings(settings: AppSettings) {
        settingsStore.save(settings)
        _settings.value = settingsStore.load()
    }
}
