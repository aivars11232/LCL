package io.lcl.workspace.remote

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec
import javax.security.auth.x500.X500Principal

/** Where device identities live. One key per paired PC. */
interface IdentityStore {
    fun load(alias: String): DeviceIdentity?
    fun create(alias: String, subject: String): DeviceIdentity
    fun delete(alias: String)
}

/**
 * Device keys in Android Keystore.
 *
 * Each paired PC gets its own ECDSA P-256 key, generated inside the keystore
 * and never exportable; the app only ever holds a handle to it. The keystore
 * also makes the self-signed certificate the PC pins. Forgetting a PC deletes
 * its key, so the pairing cannot be revived from anything left behind.
 *
 * The key is kept across app updates signed with the same key, which is what
 * keeps pairings alive through an update. It is not backed up: a restored
 * copy of the app on another phone must pair again.
 */
class KeystoreIdentities : IdentityStore {
    private fun keystore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

    override fun load(alias: String): DeviceIdentity? {
        val entry = keystore().getEntry(alias, null) as? KeyStore.PrivateKeyEntry ?: return null
        val certificate = entry.certificate as? X509Certificate ?: return null
        return DeviceIdentity(certificate, entry.privateKey)
    }

    override fun create(alias: String, subject: String): DeviceIdentity {
        delete(alias)
        val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, PROVIDER)
        generator.initialize(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                // TLS signs with the key directly (NONE) or through SHA-256.
                .setDigests(KeyProperties.DIGEST_NONE, KeyProperties.DIGEST_SHA256)
                .setCertificateSubject(X500Principal("CN=$subject"))
                .setUserAuthenticationRequired(false)
                .build(),
        )
        generator.generateKeyPair()
        return load(alias) ?: error("the keystore did not keep the new key")
    }

    override fun delete(alias: String) {
        val keystore = keystore()
        if (keystore.containsAlias(alias)) keystore.deleteEntry(alias)
    }

    private companion object {
        const val PROVIDER = "AndroidKeyStore"
    }
}
