package io.lcl.workspace.remote

import android.annotation.SuppressLint
import java.net.Socket
import java.security.PrivateKey
import java.security.Principal
import java.security.SecureRandom
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLEngine
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.X509ExtendedKeyManager
import javax.net.ssl.X509ExtendedTrustManager

/**
 * This device's identity for one PC: the certificate the PC pinned when it
 * paired, and the private key behind it. On a phone the key lives in Android
 * Keystore and never leaves it; see [KeystoreIdentities].
 */
class DeviceIdentity(val certificate: X509Certificate, val privateKey: PrivateKey) {
    val fingerprint: String get() = sha256Hex(certificate.encoded)
}

/**
 * TLS 1.3 with both ends pinned, and nothing else trusted.
 *
 * The PC is accepted only if the SHA-256 of its exact certificate is the
 * fingerprint the pairing QR code carried. No certificate authority, host
 * name or address takes part, so the PC is the same PC on any network and a
 * different machine at its address is refused. The device proves itself with
 * its own certificate, whose key signs the handshake inside the keystore.
 */
object Tls {
    const val ALPN = "lcl.remote/1"

    fun socketFactory(identity: DeviceIdentity, pinnedPc: String): SSLSocketFactory {
        val context = SSLContext.getInstance("TLSv1.3")
        context.init(arrayOf(DeviceKey(identity)), arrayOf(PinnedPc(pinnedPc)), SecureRandom())
        return context.socketFactory
    }
}

private const val ALIAS = "device"

private class DeviceKey(private val identity: DeviceIdentity) : X509ExtendedKeyManager() {
    override fun getClientAliases(keyType: String?, issuers: Array<out Principal>?) = arrayOf(ALIAS)
    override fun chooseClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, socket: Socket?) = ALIAS
    override fun chooseEngineClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, engine: SSLEngine?) = ALIAS
    override fun getServerAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? = null
    override fun chooseServerAlias(keyType: String?, issuers: Array<out Principal>?, socket: Socket?): String? = null
    override fun getCertificateChain(alias: String?): Array<X509Certificate> = arrayOf(identity.certificate)
    override fun getPrivateKey(alias: String?): PrivateKey = identity.privateKey
}

/**
 * Accepts exactly one certificate: the pinned PC's. Lint's warning about custom
 * trust managers is about ones that accept too much; this one accepts less
 * than any certificate authority would — one fingerprint, nothing else — and
 * refuses every client (see PinningTest).
 */
@SuppressLint("CustomX509TrustManager")
class PinnedPc(private val fingerprint: String) : X509ExtendedTrustManager() {
    private fun verify(chain: Array<out X509Certificate>?) {
        val presented = chain?.firstOrNull() ?: throw CertificateException("the PC presented no certificate")
        if (!constantTimeEquals(sha256Hex(presented.encoded), fingerprint)) {
            throw CertificateException("this is not the PC this device paired with")
        }
    }

    override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) = verify(chain)
    override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?, socket: Socket?) = verify(chain)
    override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?, engine: SSLEngine?) = verify(chain)
    override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?): Unit =
        throw CertificateException("this app is never a server")
    override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?, socket: Socket?): Unit =
        throw CertificateException("this app is never a server")
    override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?, engine: SSLEngine?): Unit =
        throw CertificateException("this app is never a server")
    override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
}
