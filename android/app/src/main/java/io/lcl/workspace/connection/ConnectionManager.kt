package io.lcl.workspace.connection

import io.lcl.workspace.data.PcRecord
import io.lcl.workspace.data.PcStore
import io.lcl.workspace.data.RetiringKey
import io.lcl.workspace.remote.IdentityStore
import io.lcl.workspace.remote.Opened
import io.lcl.workspace.remote.PairingLink
import io.lcl.workspace.remote.PairingVerification
import io.lcl.workspace.remote.Protocol
import io.lcl.workspace.remote.RemoteException
import io.lcl.workspace.remote.Reply
import io.lcl.workspace.remote.Session
import io.lcl.workspace.remote.Transport
import io.lcl.workspace.remote.bool
import io.lcl.workspace.remote.constantTimeEquals
import io.lcl.workspace.remote.obj
import io.lcl.workspace.remote.str
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonObject
import java.security.cert.CertificateException
import java.util.UUID
import javax.net.ssl.SSLException

/**
 * Where the connection to the active PC stands.
 *
 * Pairing and connection are different things. Pairing is trust, kept until
 * someone ends it on purpose: Forget on this device, or Revoke on the PC.
 * A connection is a socket, and sockets end — the app closes, the phone
 * sleeps, the network changes. None of that touches the pairing, and
 * reconnecting never needs a QR code.
 */
sealed interface ConnectionState {
    data object NoPc : ConnectionState
    data class Connecting(val pc: PcRecord, val attempt: Int) : ConnectionState
    data class Connected(val pc: PcRecord, val session: Session, val address: String) : ConnectionState
    data class Reconnecting(val pc: PcRecord, val reason: String, val retryInSeconds: Int) : ConnectionState
    /** No network at all. Retried as soon as there is one. */
    data class Offline(val pc: PcRecord) : ConnectionState
    /** Disconnected by the user. Still paired; Reconnect resumes. */
    data class Disconnected(val pc: PcRecord) : ConnectionState
    /** The PC revoked this device. Only a new pairing restores trust. */
    data class Revoked(val pc: PcRecord, val message: String) : ConnectionState
    /** The PC no longer knows this device (its registry was reset). */
    data class NotPaired(val pc: PcRecord, val message: String) : ConnectionState

    val pcOrNull: PcRecord?
        get() = when (this) {
            NoPc -> null
            is Connecting -> pc
            is Connected -> pc
            is Reconnecting -> pc
            is Offline -> pc
            is Disconnected -> pc
            is Revoked -> pc
            is NotPaired -> pc
        }
}

/** A pairing request the PC has not decided on yet, as the Pair screen shows it. */
data class PendingPairing(
    val pcName: String,
    /** This device's verification code, `abcd-ef12-3456`; the PC shows the same. */
    val verification: String,
    val request: String,
    val expires: Long,
)

/** How often a device waiting for approval asks the PC again. */
const val PAIRING_POLL_MS = 2_000L

/** How long retiring one earlier key may take: connecting with it, and the PC's answer. */
const val RETIRE_WITHIN_MS = 15_000L

/** Every device key this app makes is named with this, then the PC's id. */
private const val KEY_PREFIX = "lcl-pc-"

/** Whether there is a network, and a signal when the network changes. */
interface NetworkMonitor {
    val available: StateFlow<Boolean>
    /** Emits when the default network becomes a different one. */
    val changes: SharedFlow<Unit>
}

class ConnectionManager(
    private val store: PcStore,
    private val identities: IdentityStore,
    private val transport: Transport,
    private val network: NetworkMonitor,
    private val scope: CoroutineScope,
    private val discover: suspend (String) -> List<String>,
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
    /** Seconds to wait before retry number n. */
    private val backoff: (Int) -> Int = { n -> listOf(1, 2, 4, 8, 15, 30)[minOf(n, 6) - 1] },
) {
    private val _state = MutableStateFlow<ConnectionState>(ConnectionState.NoPc)
    val state: StateFlow<ConnectionState> = _state

    private val _events = MutableSharedFlow<JsonObject>(extraBufferCapacity = 512)
    /** Events from whichever session is live, in order. */
    val events: SharedFlow<JsonObject> = _events

    private val _records = MutableStateFlow(store.all())
    /** The PCs this device is paired with, as saved; it follows every change made here. */
    val records: StateFlow<List<PcRecord>> = _records

    private var loop: Job? = null
    private val wake = Channel<Unit>(Channel.CONFLATED)
    /** One retirement at a time (see [retirePrevious]), so two never race over a key or a record. */
    private val retirements = Mutex()

    init {
        scope.launch { network.changes.collect { wake.trySend(Unit) } }
    }

    fun pcs(): List<PcRecord> = store.all()

    private fun save(record: PcRecord) {
        store.save(record)
        _records.value = store.all()
    }

    /** Change a PC's record as it is saved now; nothing if it is gone. */
    private fun update(pcId: String, change: (PcRecord) -> PcRecord) {
        store.get(pcId)?.let(change)?.let(::save)
    }

    /**
     * Resume the active PC, as the app does on every start. A key no saved PC
     * uses or is retiring — made for a pairing the process did not live to
     * finish — is deleted first.
     */
    fun start() {
        val used = store.all().flatMap { pc -> listOf(pc.keyAlias) + pc.retiring.map { it.keyAlias } }.toSet()
        for (alias in runCatching { identities.aliases() }.getOrDefault(emptyList())) {
            if (alias.startsWith(KEY_PREFIX) && alias !in used) runCatching { identities.delete(alias) }
        }
        val pc = store.activeId()?.let(store::get) ?: return
        connect(pc.pcId)
    }

    /** Work with this PC from now on, reconnecting to it whenever possible. */
    fun connect(pcId: String) {
        val pc = store.get(pcId) ?: return
        store.setActive(pcId)
        startLoop(pc, null)
    }

    /** Try again now instead of waiting out the backoff. */
    fun reconnectNow() {
        val pc = _state.value.pcOrNull ?: return
        if (loop?.isActive == true) wake.trySend(Unit) else startLoop(pc, null)
    }

    /** End the live connection. The pairing stays; Reconnect resumes it. */
    fun disconnect() {
        val pc = _state.value.pcOrNull ?: return
        stop()
        _state.value = ConnectionState.Disconnected(pc)
    }

    /**
     * Forget a PC: its trust and its keys are deleted, and only a new QR code
     * pairs it again. If the PC is connected it is also asked to stop trusting
     * this device; forgetting never waits for that, and succeeds without it.
     * An earlier key the PC may still trust ([PcRecord.retiring]) asks it once
     * more to stop trusting it, then is deleted whether the PC answered or not.
     */
    fun forget(pcId: String) {
        val pc = store.get(pcId) ?: return
        var live: Session? = null
        if (_state.value.pcOrNull?.pcId == pcId) {
            live = (_state.value as? ConnectionState.Connected)?.session
            loop?.cancel() // the live session stays open just long enough to say goodbye
            loop = null
            _state.value = ConnectionState.NoPc
        }
        runCatching { identities.delete(pc.keyAlias) }
        store.remove(pcId)
        _records.value = store.all()
        live?.let { session ->
            scope.launch {
                runCatching { withTimeout(3_000) { session.request("unpair") } }
                session.close()
            }
        }
        if (pc.retiring.isNotEmpty()) {
            scope.launch {
                retirements.withLock {
                    for (key in pc.retiring) {
                        retire(pc, key)
                        runCatching { identities.delete(key.keyAlias) }
                    }
                }
            }
        }
    }

    /** One request to the connected PC. */
    suspend fun request(op: String, fields: JsonObject = JsonObject(emptyMap())): Reply {
        val connected = _state.value as? ConnectionState.Connected
            ?: throw RemoteException("Not connected to the PC.")
        return connected.session.request(op, fields)
    }

    /**
     * Pair with the PC a QR code names.
     *
     * The code only lets this device ask. A fresh key is made for this
     * attempt, and the PC records a pending request for exactly that key; the
     * person at the PC approves it after comparing the verification code,
     * which [onPending] hands to the screen. Until then nothing is saved and
     * the active PC is left alone; the PC is asked again every
     * [PAIRING_POLL_MS]. Once approved, the PC is saved and becomes the active
     * one, already connected.
     *
     * Denied, expired, refused or cancelled — the coroutine is cancelled when
     * the person presses Cancel or leaves the screen — the new key is deleted
     * and nothing is saved. An older pairing with the same PC is replaced only
     * once this one has succeeded, so a failed attempt breaks nothing.
     *
     * Replacing it ends only when the PC trusts the older key no more. The PC
     * keeps a record per key, so it now trusts both, and only the older key
     * itself can end its record. The new pairing is saved with the older key
     * listed in [PcRecord.retiring]; that key then asks the PC to stop
     * trusting it ([retirePrevious]), and is deleted only once the PC is shown
     * to trust it no more. When that cannot be shown now, the key stays
     * listed, with why, and is tried again each time this PC is connected: the
     * record returned says so, and a replacement is never reported finished
     * before it is.
     */
    suspend fun pair(link: PairingLink, deviceName: String, onPending: (PendingPairing) -> Unit = {}): Result<PcRecord> {
        if (link.isExpired(now())) {
            return Result.failure(RemoteException("This QR code has expired. Show a new one on the PC."))
        }
        val alias = "$KEY_PREFIX${link.pcId}-${UUID.randomUUID()}"
        val identity = runCatching { identities.create(alias, "LCL Android") }.getOrElse {
            return Result.failure(RemoteException("This device could not make a key: ${it.message}"))
        }
        // What this device shows is computed here, from its own certificate;
        // the PC must show the same.
        val verification = PairingVerification.of(link.fingerprint, link.code, identity.fingerprint)
        val hello = Protocol.helloPair(link.code, deviceName)
        fun failed(message: String): Result<PcRecord> {
            runCatching { identities.delete(alias) }
            return Result.failure(RemoteException(message))
        }
        var lastError = "Could not reach the PC."
        // The address that answered with a pending request, tried first from then on.
        var answering: String? = null
        var expires = link.expires
        // Once saved, the new key is the pairing's own, not this attempt's to delete.
        var saved = false
        try {
            while (true) {
                val order = answering?.let { listOf(it) + (link.addresses - it) } ?: link.addresses
                var waiting = false
                for (address in order) {
                    val opened = try {
                        transport.open(address, link.fingerprint, identity, hello)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        lastError = describe(e, address, pairing = true)
                        continue
                    }
                    when (opened) {
                        is Opened.Refused -> return failed(
                            when (opened.code) {
                                "pairing_denied" -> "The PC denied this device. Nothing was paired."
                                "pairing_upgrade_required", "unsupported_pairing_version" ->
                                    "The PC and this app pair differently. Update LCL on both, then show a new QR code."
                                else -> opened.message
                            },
                        )
                        is Opened.Pending -> {
                            if (!constantTimeEquals(opened.verification, verification)) {
                                return failed("The PC's verification code does not match this device's. Nothing was paired; show a new QR code.")
                            }
                            answering = address
                            expires = minOf(expires, opened.expires)
                            onPending(PendingPairing(opened.pcName ?: link.pcName, verification, opened.request, expires))
                            waiting = true
                        }
                        is Opened.Accepted -> {
                            val device = opened.paired?.obj("device")
                            val pcInfo = opened.session.welcome.obj("pc")
                            val previous = store.get(link.pcId)
                            val record = PcRecord(
                                pcId = link.pcId,
                                name = pcInfo?.str("name") ?: link.pcName,
                                fingerprint = link.fingerprint,
                                addresses = link.addresses,
                                deviceId = device?.str("id") ?: opened.session.welcome.obj("device")?.str("id") ?: "",
                                keyAlias = alias,
                                pairedAt = now(),
                                lastConnected = now(),
                                lastAddress = address,
                                // The key used for this PC until now stays until the PC trusts it no more.
                                retiring = previous?.let { it.retiring + RetiringKey(it.keyAlias, it.deviceId) }
                                    ?.filter { it.keyAlias != alias }?.distinctBy { it.keyAlias }.orEmpty(),
                            )
                            save(record)
                            store.setActive(record.pcId)
                            saved = true
                            startLoop(record, opened.session to address)
                            if (record.retiring.isEmpty()) return Result.success(record)
                            // In the app's own scope, so leaving the Pair screen does not stop it.
                            return Result.success(scope.async { retirePrevious(record.pcId) }.await() ?: record)
                        }
                    }
                    if (waiting) break
                }
                // Never reached: nothing to wait for. Reached before and not
                // now: the network may be back in a moment, so keep asking
                // until the code expires.
                if (answering == null) return failed(lastError)
                if (now() >= expires) return failed("The pairing request expired before the PC approved it. Show a new QR code on the PC.")
                delay(PAIRING_POLL_MS)
                if (now() >= expires) return failed("The pairing request expired before the PC approved it. Show a new QR code on the PC.")
            }
        } catch (e: CancellationException) {
            if (!saved) runCatching { identities.delete(alias) }
            throw e
        }
    }

    private fun describe(e: Exception, address: String, pairing: Boolean = false): String = when {
        e is CertificateException || e.cause is CertificateException ||
            (e is SSLException && e.message?.contains("paired", ignoreCase = true) == true) ->
            if (pairing) "The computer at $address is not the PC in this QR code."
            else "The computer at $address is not the PC this device paired with."
        e is SSLException -> "A secure connection to $address failed: ${e.message}"
        else -> "Could not reach the PC at $address: ${e.message ?: e.javaClass.simpleName}"
    }

    private fun stop() {
        loop?.cancel()
        loop = null
        (_state.value as? ConnectionState.Connected)?.session?.close()
    }

    /** The connection loop for one PC: connect, stay connected, reconnect. */
    private fun startLoop(pc: PcRecord, adopted: Pair<Session, String>?) {
        stop()
        loop = scope.launch {
            var attempt = 0
            var reason = ""
            var handed = adopted
            while (isActive) {
                val current = store.get(pc.pcId) ?: pc
                // A session pair() hands over: it finishes its own retirements.
                val fromPairing = handed != null
                val connected: Pair<Session, String>? = handed ?: run {
                    if (!network.available.value) {
                        _state.value = ConnectionState.Offline(current)
                        network.available.first { it }
                    }
                    attempt += 1
                    _state.value = ConnectionState.Connecting(current, attempt)
                    when (val outcome = connectOnce(current)) {
                        is Outcome.Live -> outcome.session to outcome.address
                        is Outcome.Refused -> {
                            _state.value = if (outcome.code == "revoked") {
                                ConnectionState.Revoked(current, outcome.message)
                            } else {
                                ConnectionState.NotPaired(current, outcome.message)
                            }
                            return@launch
                        }
                        is Outcome.Unreachable -> {
                            reason = outcome.reason
                            null
                        }
                    }
                }
                handed = null
                if (connected != null) {
                    val (session, address) = connected
                    attempt = 0
                    // The record as saved now: a retirement may have changed it while this connected.
                    val latest = store.get(pc.pcId) ?: current
                    val updated = latest.copy(
                        lastConnected = now(),
                        lastAddress = address,
                        addresses = (listOf(address) + latest.addresses).distinct().take(8),
                    )
                    save(updated)
                    _state.value = ConnectionState.Connected(updated, session, address)
                    // The PC can be reached: earlier keys it may still trust are tried again.
                    if (!fromPairing && updated.retiring.isNotEmpty()) launch { retirePrevious(pc.pcId) }
                    val forwarding = launch { session.events.collect { _events.emit(it) } }
                    val revoked = launch {
                        session.events.first { it.str("event") == "revoked" }
                        _state.value = ConnectionState.Revoked(updated, "This PC revoked this device. Pair it again with a new QR code.")
                        session.close()
                    }
                    // Stay until the connection ends or the network changes under it.
                    val networkChanged = coroutineScope {
                        val ended = async { session.closed.await(); false }
                        val changed = async { wake.receive(); true }
                        select { ended.onAwait { it }; changed.onAwait { it } }.also {
                            ended.cancel()
                            changed.cancel()
                        }
                    }
                    forwarding.cancel()
                    session.close()
                    if (revoked.isCompleted) return@launch
                    revoked.cancel()
                    reason = if (networkChanged) "The network changed." else "The connection to the PC ended."
                    if (networkChanged) continue // retry at once, on the new network
                    attempt = 1
                }
                val wait = backoff(maxOf(attempt, 1))
                _state.value = ConnectionState.Reconnecting(current, reason, wait)
                withTimeoutOrNull(wait * 1000L) { wake.receive() }
            }
        }
    }

    private sealed interface Outcome {
        data class Live(val session: Session, val address: String) : Outcome
        data class Refused(val code: String, val message: String) : Outcome
        data class Unreachable(val reason: String) : Outcome
    }

    /**
     * Retire the earlier keys a PC's record lists (see [pair]), one at a time.
     * A key the PC is shown to trust no more is taken off the list, then
     * deleted; one that is not stays listed, with why. The record as saved
     * afterwards, or `null` if the PC was forgotten meanwhile ([forget] then
     * sees to its keys).
     */
    private suspend fun retirePrevious(pcId: String): PcRecord? = retirements.withLock {
        for (key in store.get(pcId)?.retiring.orEmpty()) {
            val pc = store.get(pcId) ?: break
            when (val outcome = retire(pc, key)) {
                Retirement.Done -> {
                    update(pcId) { it.copy(retiring = it.retiring.filterNot { k -> k.keyAlias == key.keyAlias }) }
                    runCatching { identities.delete(key.keyAlias) }
                }
                is Retirement.NotYet -> update(pcId) { record ->
                    record.copy(retiring = record.retiring.map { if (it.keyAlias == key.keyAlias) it.copy(problem = outcome.problem) else it })
                }
            }
        }
        store.get(pcId)
    }

    private sealed interface Retirement {
        /** The PC does not trust the key: it revoked it now, or had revoked it or never known it. */
        data object Done : Retirement
        data class NotYet(val problem: String) : Retirement
    }

    /**
     * Ask the PC to stop trusting one earlier key of this device. The key
     * connects as itself and sends `unpair`: the PC revokes the record of the
     * certificate that connection proved, so exactly that record ends, and no
     * other can be named. Only the PC's own answer is proof — `unpaired`, or
     * the key refused as revoked or unknown; anything else is not.
     */
    private suspend fun retire(pc: PcRecord, key: RetiringKey): Retirement {
        val identity = runCatching { identities.load(key.keyAlias) }.getOrNull()
            ?: return Retirement.NotYet("this device no longer holds that key, so only the PC can end its trust")
        // A key with the certificate the pairing uses now is not an earlier
        // one: the PC keeps one record per certificate, and unpairing with it
        // would end this pairing.
        val current = runCatching { identities.load(pc.keyAlias)?.fingerprint }.getOrNull()
        if (current != null && constantTimeEquals(current, identity.fingerprint)) return Retirement.Done
        return withTimeoutOrNull(RETIRE_WITHIN_MS) {
            var problem = "the PC could not be reached"
            for (address in (listOfNotNull(pc.lastAddress) + pc.addresses).distinct()) {
                val opened = try {
                    transport.open(address, pc.fingerprint, identity, Protocol.helloConnect(null))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    problem = describe(e, address)
                    continue
                }
                return@withTimeoutOrNull when (opened) {
                    is Opened.Refused ->
                        if (opened.code in setOf("revoked", "not_paired")) Retirement.Done else Retirement.NotYet(opened.message)
                    is Opened.Pending -> Retirement.NotYet("the PC answered as if this device were pairing")
                    is Opened.Accepted -> try {
                        val reply = opened.session.request("unpair")
                        if (reply.ok && reply.obj.bool("unpaired") == true) {
                            Retirement.Done
                        } else {
                            Retirement.NotYet(reply.error ?: "the PC answered ${reply.status}")
                        }
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        Retirement.NotYet("the connection ended before the PC answered")
                    } finally {
                        opened.session.close()
                    }
                }
            }
            Retirement.NotYet(problem)
        } ?: Retirement.NotYet("the PC did not answer in time")
    }

    /** Try every address this PC might be at: last good one first, then the rest, then the local network. */
    private suspend fun connectOnce(pc: PcRecord): Outcome {
        val identity = runCatching { identities.load(pc.keyAlias) }.getOrNull()
            ?: return Outcome.Refused("no_key", "This device's key for ${pc.name} is gone. Forget the PC and pair it again.")
        val hello = Protocol.helloConnect(pc.deviceId)
        var reason = "The PC could not be reached."
        val tried = mutableSetOf<String>()
        suspend fun attempt(addresses: List<String>): Outcome? {
            for (address in addresses) {
                if (!tried.add(address)) continue
                try {
                    when (val opened = transport.open(address, pc.fingerprint, identity, hello)) {
                        is Opened.Accepted -> return Outcome.Live(opened.session, address)
                        is Opened.Refused -> if (opened.code in setOf("revoked", "not_paired", "identity_mismatch")) {
                            return Outcome.Refused(opened.code, opened.message)
                        } else {
                            reason = opened.message
                        }
                        is Opened.Pending -> reason = "The PC answered as if this device were pairing."
                    }
                } catch (e: CancellationException) {
                    throw e // Disconnect or Forget: stop here, and say nothing more
                } catch (e: Exception) {
                    reason = describe(e, address)
                }
            }
            return null
        }
        val known = listOfNotNull(pc.lastAddress) + pc.addresses
        attempt(known)?.let { return it }
        val found = try {
            discover(pc.pcId)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            emptyList()
        }
        attempt(found)?.let { return it }
        return Outcome.Unreachable(reason)
    }
}
