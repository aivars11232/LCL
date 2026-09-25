package io.lcl.workspace.remote

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import javax.net.ssl.SSLSocket

/** A live, authenticated connection to one PC. */
interface Session {
    /** What the PC said when it let this device in. */
    val welcome: JsonObject
    /** Messages the PC sends on its own: run progress, changed documents, revocation. */
    val events: SharedFlow<JsonObject>
    /** Completes when the connection ends, with why. */
    val closed: Deferred<Throwable?>
    suspend fun request(op: String, fields: JsonObject = JsonObject(emptyMap())): Reply
    fun close()
}

/** How a connection attempt ended. */
sealed interface Opened {
    /** Let in. `paired` is set when this connection paired the device. */
    data class Accepted(val session: Session, val paired: JsonObject?) : Opened
    /** Refused by the PC, with its reason: not_paired, revoked, pairing_refused, pairing_denied, … */
    data class Refused(val code: String, val message: String) : Opened
    /**
     * A pairing request the PC recorded and has not decided on. Nothing is
     * trusted, and the PC closed the connection; ask again later.
     */
    data class Pending(val request: String, val verification: String, val expires: Long, val pcName: String?) : Opened
}

/** Opens connections. The real one is [TlsTransport]; tests substitute their own. */
interface Transport {
    suspend fun open(address: String, pin: String, identity: DeviceIdentity, hello: JsonObject): Opened
}

/** Split `host:port`, `[v6]:port` included. */
fun splitAddress(address: String): Pair<String, Int> {
    val at = address.lastIndexOf(':')
    require(at > 0) { "$address has no port" }
    val host = address.substring(0, at).removePrefix("[").removeSuffix("]")
    val port = address.substring(at + 1).toIntOrNull()?.takeIf { it in 1..65535 }
        ?: throw IllegalArgumentException("$address has no valid port")
    return host to port
}

class TlsTransport(private val connectTimeoutMs: Int = 4_000) : Transport {
    override suspend fun open(address: String, pin: String, identity: DeviceIdentity, hello: JsonObject): Opened {
        // A blocking connect finishes even if the caller gave up meanwhile
        // (Disconnect, Forget); a session it opened then must not live on.
        var opened: Opened? = null
        try {
            return withContext(Dispatchers.IO) { connect(address, pin, identity, hello).also { opened = it } }
        } catch (e: CancellationException) {
            (opened as? Opened.Accepted)?.session?.close()
            throw e
        }
    }

    private fun connect(address: String, pin: String, identity: DeviceIdentity, hello: JsonObject): Opened {
        val (host, port) = splitAddress(address)
        val raw = Socket()
        try {
            raw.connect(InetSocketAddress(host, port), connectTimeoutMs)
            raw.tcpNoDelay = true
            raw.soTimeout = 15_000 // the handshake and the answer to hello, bounded
            val socket = Tls.socketFactory(identity, pin).createSocket(raw, host, port, true) as SSLSocket
            socket.enabledProtocols = arrayOf("TLSv1.3")
            socket.sslParameters = socket.sslParameters.apply { applicationProtocols = arrayOf(Tls.ALPN) }
            socket.startHandshake()
            val input = DataInputStream(BufferedInputStream(socket.inputStream))
            val output = DataOutputStream(BufferedOutputStream(socket.outputStream))
            Frames.write(output, hello.toString())
            var first = Protocol.parse(Frames.read(input))
            var paired: JsonObject? = null
            if (first.str("type") == "paired") {
                paired = first
                first = Protocol.parse(Frames.read(input))
            }
            return when (first.str("type")) {
                "welcome" -> {
                    socket.soTimeout = 0
                    val session = TlsSession(socket, input, output, first)
                    session.start()
                    Opened.Accepted(session, paired)
                }
                "error" -> {
                    socket.close()
                    Opened.Refused(first.str("code") ?: "error", first.str("message") ?: "the PC refused")
                }
                "pairing_pending" -> {
                    socket.close()
                    Opened.Pending(
                        request = first.str("request") ?: throw IOException("the PC's pending answer names no request"),
                        verification = first.str("verification") ?: throw IOException("the PC's pending answer has no verification code"),
                        expires = first.long("expires") ?: throw IOException("the PC's pending answer has no expiry"),
                        pcName = first.obj("pc")?.str("name"),
                    )
                }
                else -> {
                    socket.close()
                    throw IOException("the PC answered hello with ${first.str("type")}")
                }
            }
        } catch (e: Exception) {
            runCatching { raw.close() }
            throw e
        }
    }
}

private class TlsSession(
    private val socket: SSLSocket,
    private val input: DataInputStream,
    private val output: DataOutputStream,
    override val welcome: JsonObject,
) : Session {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val next = AtomicLong(1)
    private val pending = ConcurrentHashMap<Long, CompletableDeferred<Reply>>()
    private val _events = MutableSharedFlow<JsonObject>(extraBufferCapacity = 512)
    override val events: SharedFlow<JsonObject> = _events
    override val closed = CompletableDeferred<Throwable?>()

    fun start() {
        scope.launch {
            val reason = runCatching {
                while (isActive) {
                    val message = Protocol.parse(Frames.read(input))
                    when (message.str("type")) {
                        "response" -> {
                            val id = message.long("id") ?: continue
                            val status = message.long("status")?.toInt() ?: 0
                            pending.remove(id)?.complete(Reply(status, message["body"] ?: JsonNull))
                        }
                        "event" -> _events.emit(message)
                        else -> throw IOException("the PC sent a ${message.str("type")} message mid-session")
                    }
                }
            }.exceptionOrNull()
            finish(reason ?: IOException("the connection ended"))
        }
        // Keep the connection honest: a PC that stops answering is noticed
        // within one missed ping, and the PC's own idle limit never trips.
        scope.launch {
            while (isActive) {
                delay(20_000)
                val alive = runCatching { withTimeout(10_000) { request("ping") } }.isSuccess
                if (!alive) finish(IOException("the PC stopped answering"))
            }
        }
    }

    private fun finish(reason: Throwable) {
        if (closed.complete(reason)) {
            runCatching { socket.close() }
            val error = RemoteException("the connection to the PC ended")
            pending.values.forEach { it.completeExceptionally(error) }
            pending.clear()
            scope.cancel()
        }
    }

    override suspend fun request(op: String, fields: JsonObject): Reply {
        if (closed.isCompleted) throw RemoteException("not connected to the PC")
        val id = next.getAndIncrement()
        val answer = CompletableDeferred<Reply>()
        pending[id] = answer
        try {
            withContext(Dispatchers.IO) { Frames.write(output, Protocol.request(id, op, fields).toString()) }
        } catch (e: IOException) {
            pending.remove(id)
            finish(e)
            throw RemoteException("the connection to the PC ended")
        }
        return withTimeout(60_000) { answer.await() }
    }

    override fun close() = finish(IOException("closed by this device"))
}
