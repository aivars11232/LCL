package io.lcl.workspace.connection

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * The phone's network, as the connection loop needs it: whether there is one,
 * and a signal when the default network becomes a different one — Wi-Fi to
 * mobile data, one Wi-Fi to another — so a reconnect starts at once instead
 * of waiting for a dead socket to time out.
 */
class AndroidNetwork(context: Context) : NetworkMonitor {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val _available = MutableStateFlow(connectivity.activeNetwork != null)
    override val available: StateFlow<Boolean> = _available
    private val _changes = MutableSharedFlow<Unit>(extraBufferCapacity = 4)
    override val changes: SharedFlow<Unit> = _changes
    private var current: Network? = connectivity.activeNetwork

    init {
        connectivity.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                val changed = current != null && current != network
                current = network
                _available.value = true
                if (changed) _changes.tryEmit(Unit)
            }

            override fun onLost(network: Network) {
                if (current == network) current = null
                _available.value = connectivity.activeNetwork != null
                _changes.tryEmit(Unit)
            }
        })
    }
}
