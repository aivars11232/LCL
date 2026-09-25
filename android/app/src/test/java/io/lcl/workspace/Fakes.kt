package io.lcl.workspace

import io.lcl.workspace.connection.NetworkMonitor
import io.lcl.workspace.remote.DeviceIdentity
import io.lcl.workspace.remote.IdentityStore
import io.lcl.workspace.remote.Opened
import io.lcl.workspace.remote.PairingLink
import io.lcl.workspace.remote.Reply
import io.lcl.workspace.remote.Session
import io.lcl.workspace.remote.Transport
import io.lcl.workspace.remote.str
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Paths
import java.security.KeyStore
import java.security.PrivateKey
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.Base64
import javax.net.ssl.SSLHandshakeException

/**
 * A real certificate and EC key, made once per test run by the JDK's own
 * keytool, so no key file is ever part of the source. It lives only in this
 * test process; the temporary keystore it came from is deleted at once.
 */
private val identity: DeviceIdentity by lazy {
    val dir = Files.createTempDirectory("lcl-test-identity")
    val store = dir.resolve("device.p12")
    val keytool = Paths.get(System.getProperty("java.home"), "bin", "keytool").toString()
    val process = ProcessBuilder(
        keytool, "-genkeypair", "-alias", "device", "-keyalg", "EC", "-groupname", "secp256r1",
        "-sigalg", "SHA256withECDSA", "-dname", "CN=LCL test device", "-validity", "1",
        "-keystore", store.toString(), "-storetype", "PKCS12", "-storepass", "test-only", "-keypass", "test-only",
    ).redirectErrorStream(true).start()
    val output = process.inputStream.bufferedReader().readText()
    check(process.waitFor() == 0) { "keytool could not make the test identity: $output" }
    val keystore = KeyStore.getInstance("PKCS12")
    Files.newInputStream(store).use { keystore.load(it, "test-only".toCharArray()) }
    Files.delete(store)
    Files.delete(dir)
    DeviceIdentity(keystore.getCertificate("device") as X509Certificate, keystore.getKey("device", "test-only".toCharArray()) as PrivateKey)
}

fun testIdentity(): DeviceIdentity = identity

class FakeIdentities : IdentityStore {
    val keys = mutableMapOf<String, DeviceIdentity>()
    val deleted = mutableListOf<String>()
    override fun load(alias: String) = keys[alias]
    override fun create(alias: String, subject: String) = testIdentity().also { keys[alias] = it }
    override fun delete(alias: String) {
        if (keys.remove(alias) != null) deleted += alias
    }
}

class FakeNetwork : NetworkMonitor {
    override val available = MutableStateFlow(true)
    override val changes = MutableSharedFlow<Unit>(extraBufferCapacity = 4)
}

/** One session with a scripted PC behind it. */
class FakeSession(
    override val welcome: JsonObject,
    private val answer: suspend (String, JsonObject) -> Reply,
) : Session {
    val requests = mutableListOf<Pair<String, JsonObject>>()
    private val _events = MutableSharedFlow<JsonObject>(extraBufferCapacity = 64)
    override val events: SharedFlow<JsonObject> = _events
    override val closed = CompletableDeferred<Throwable?>()

    suspend fun emit(event: JsonObject) = _events.emit(event)

    override suspend fun request(op: String, fields: JsonObject): Reply {
        if (closed.isCompleted) throw IOException("closed")
        requests += op to fields
        return answer(op, fields)
    }

    override fun close() {
        closed.complete(null)
    }

    /** The PC went away. */
    fun drop() {
        closed.complete(IOException("dropped"))
    }
}

/**
 * A PC reachable at some addresses, with a fingerprint, the devices it
 * trusts, and a pairing code it accepts once.
 */
class FakePc(
    val pcId: String = "pc1",
    val name: String = "Test PC",
    val fingerprint: String = "a".repeat(64),
    var reachable: Set<String> = setOf("192.168.1.20:47300"),
    var code: String? = null,
) : Transport {
    /** Trusted devices: certificate fingerprint to the id this PC gave it. */
    val devices = mutableMapOf<String, String>()
    val revoked = mutableSetOf<String>()
    val sessions = mutableListOf<FakeSession>()
    val hellos = mutableListOf<JsonObject>()
    var attempts = 0
    /** When set, a connection attempt waits on it: a slow network, mid-handshake. */
    var hold: CompletableDeferred<Unit>? = null
    var answer: suspend (String, JsonObject) -> Reply = { _, _ -> Reply(200, JsonObject(emptyMap())) }
    private var nextDevice = 1

    /** Show a QR code: a fresh one-time code, good for five minutes. */
    fun link(now: Long): PairingLink {
        val fresh = Base64.getUrlEncoder().withoutPadding().encodeToString(ByteArray(32) { (nextCode + it).toByte() })
        nextCode += 1
        code = fresh
        return PairingLink(1, pcId, name, fingerprint, reachable.toList(), fresh, now + 300)
    }
    private var nextCode = 1

    override suspend fun open(address: String, pin: String, identity: DeviceIdentity, hello: JsonObject): Opened {
        attempts += 1
        hold?.await()
        if (address !in reachable) throw IOException("unreachable $address")
        if (pin != fingerprint) {
            throw SSLHandshakeException("pin").apply { initCause(CertificateException("this is not the PC this device paired with")) }
        }
        hellos += hello
        val device = identity.fingerprint
        var paired: JsonObject? = null
        when (hello.str("intent")) {
            "pair" -> {
                val offered = hello.str("code")
                if (offered == null || offered != code) {
                    return Opened.Refused("pairing_refused", "this pairing code was already used; show a new QR code")
                }
                code = null
                val id = "dev${nextDevice++}"
                devices[device] = id
                revoked -= device
                paired = buildJsonObject {
                    put("type", "paired")
                    put("device", buildJsonObject { put("id", id); put("name", hello.str("name")) })
                }
            }
            "connect" -> when (device) {
                in revoked -> return Opened.Refused("revoked", "this PC revoked this device")
                !in devices -> return Opened.Refused("not_paired", "this PC does not know this device; pair it again")
                else -> Unit
            }
            else -> return Opened.Refused("protocol", "unknown intent")
        }
        val welcome = buildJsonObject {
            put("type", "welcome")
            put("pc", buildJsonObject { put("id", pcId); put("name", name); put("fingerprint", fingerprint) })
            put("device", buildJsonObject { put("id", devices.getValue(device)) })
        }
        val session = FakeSession(welcome) { op, fields -> answer(op, fields) }
        sessions += session
        return Opened.Accepted(session, paired)
    }
}

/** A network of machines: whoever answers at an address gets the connection. */
class Lan(vararg machines: FakePc) : Transport {
    var machines: List<FakePc> = machines.toList()

    override suspend fun open(address: String, pin: String, identity: DeviceIdentity, hello: JsonObject): Opened {
        val machine = machines.firstOrNull { address in it.reachable } ?: throw IOException("nothing answers at $address")
        return machine.open(address, pin, identity, hello)
    }
}

fun reply(status: Int, vararg fields: Pair<String, Any?>): Reply = Reply(
    status,
    buildJsonObject {
        for ((k, v) in fields) when (v) {
            is String -> put(k, v)
            is Long -> put(k, v)
            is Int -> put(k, v)
            is Boolean -> put(k, v)
            is JsonElement -> put(k, v)
        }
    },
)
