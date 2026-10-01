package io.lcl.workspace.connection

import io.lcl.workspace.data.PcRecord
import io.lcl.workspace.remote.PairingLink
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

/** Where a pairing attempt stands. */
sealed interface PairingState {
    /** No attempt, or its result was acknowledged. */
    data object Idle : PairingState

    /** Pair was pressed: a key is being made and the PC asked. */
    data object Requesting : PairingState

    /** The PC recorded the request; nothing is trusted until it is approved there. */
    data class Pending(val pending: PendingPairing) : PairingState

    /** The PC approved: the pairing is being saved (and an older key retired). */
    data object Finishing : PairingState

    /** Paired and connected. */
    data class Paired(val record: PcRecord) : PairingState

    /** Paired and connected, but the PC may still trust the key this pairing replaced. */
    data class RetirementIncomplete(val record: PcRecord) : PairingState

    /**
     * Denied, expired, refused, failed or cancelled: nothing was paired and the attempt's key is
     * deleted.
     */
    data class Failed(val message: String) : PairingState
}

/**
 * The one pairing attempt of the app, owned for as long as the process lives rather than by the
 * Pair screen (C03-AUDIT-03). Leaving the screen — for the Manual tab, or through activity
 * recreation — therefore neither cancels the attempt nor loses its state; only an explicit [cancel]
 * does. Every rule of the attempt itself (PC approval, verification code, pinning, A12 retirement,
 * A13 identity) is [ConnectionManager.pair]'s, unchanged.
 */
class PairingController(
    private val connection: ConnectionManager,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow<PairingState>(PairingState.Idle)
    val state: StateFlow<PairingState> = _state
    private var attempt: Job? = null

    /** True while an attempt is running. */
    val active: Boolean
        get() = attempt?.isActive == true

    /** Start pairing with [link], unless an attempt is already running. */
    fun start(link: PairingLink, deviceName: String): Boolean {
        if (active) return false
        _state.value = PairingState.Requesting
        val job = scope.launch {
            val result =
                connection.pair(
                    link,
                    deviceName,
                    onApproved = {
                        if (_state.value !is PairingState.Failed)
                            _state.value = PairingState.Finishing
                    },
                ) { pending ->
                    _state.value = PairingState.Pending(pending)
                }
            // A cancelled attempt never returns here: cancel() already said so.
            _state.value =
                result.fold(
                    onSuccess = {
                        if (it.retiring.isEmpty()) PairingState.Paired(it)
                        else PairingState.RetirementIncomplete(it)
                    },
                    onFailure = { PairingState.Failed(it.message ?: "Pairing failed.") },
                )
        }
        attempt = job
        job.invokeOnCompletion { if (attempt === job) attempt = null }
        return true
    }

    /** The person's explicit Cancel: the attempt ends, its key is deleted, nothing is saved. */
    fun cancel() {
        val job = attempt ?: return
        job.cancel()
        _state.value = PairingState.Failed("Pairing cancelled. Nothing was paired.")
    }

    /** Acknowledge a finished attempt's result. A running attempt is not touched. */
    fun reset() {
        if (!active) _state.value = PairingState.Idle
    }
}
