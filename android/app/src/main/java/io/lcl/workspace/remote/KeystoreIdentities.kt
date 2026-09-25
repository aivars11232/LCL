package io.lcl.workspace.remote

import android.annotation.SuppressLint
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyProperties
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec
import javax.security.auth.x500.X500Principal

/** Where device identities live. One key per paired PC. */
interface IdentityStore {
    fun load(alias: String): DeviceIdentity?
    fun create(alias: String, subject: String): DeviceIdentity
    fun delete(alias: String)

    /** Every key this store holds. */
    fun aliases(): List<String> = emptyList()

    /** Where the key's material is kept, as far as the platform says; `null` if unknown. */
    fun protection(alias: String): KeyProtection? = null
}

/**
 * Where Android Keystore keeps a key's material. It depends on the phone: a
 * StrongBox secure element, a trusted execution environment, other secure
 * hardware, or the keystore in software. Every one keeps the key
 * non-exportable — the app holds only a handle — but only the hardware kinds
 * are hardware-backed, and nothing here requires one: a phone without them is
 * still supported.
 */
enum class KeyProtection(val description: String, val hardwareBacked: Boolean) {
    STRONGBOX("StrongBox secure element (hardware)", true),
    TRUSTED_ENVIRONMENT("trusted execution environment, TEE (hardware)", true),
    SECURE_HARDWARE("secure hardware, kind not reported", true),
    SOFTWARE("software keystore (not hardware-backed)", false),
    UNKNOWN("not reported by this phone", false),
    ;

    companion object {
        /**
         * From `KeyInfo.getSecurityLevel()` on Android 12 (API 31) and later;
         * before it, `securityLevel` is null and `insideSecureHardware` is all
         * the platform says.
         */
        // The constants are compile-time values; a level only arrives from API 31.
        @SuppressLint("InlinedApi")
        fun of(securityLevel: Int?, insideSecureHardware: Boolean): KeyProtection = when (securityLevel) {
            null -> if (insideSecureHardware) SECURE_HARDWARE else SOFTWARE
            KeyProperties.SECURITY_LEVEL_STRONGBOX -> STRONGBOX
            KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> TRUSTED_ENVIRONMENT
            KeyProperties.SECURITY_LEVEL_UNKNOWN_SECURE -> SECURE_HARDWARE
            KeyProperties.SECURITY_LEVEL_SOFTWARE -> SOFTWARE
            else -> UNKNOWN
        }
    }
}

/**
 * Device keys in Android Keystore.
 *
 * Each paired PC gets its own ECDSA P-256 key, generated inside the keystore
 * and never exportable; the app only ever holds a handle to it. The keystore
 * also makes the self-signed certificate the PC pins. Forgetting a PC deletes
 * its key, so the pairing cannot be revived from anything left behind.
 *
 * Whether the key is hardware-backed is the phone's to decide, and differs
 * between phones; [protection] reports what the platform says for this key.
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

    override fun aliases(): List<String> = keystore().aliases().toList()

    override fun protection(alias: String): KeyProtection? {
        val key = keystore().getKey(alias, null) as? PrivateKey ?: return null
        val info = KeyFactory.getInstance(key.algorithm, PROVIDER).getKeySpec(key, KeyInfo::class.java)
        return if (Build.VERSION.SDK_INT >= 31) {
            KeyProtection.of(info.securityLevel, insideSecureHardware = false)
        } else {
            KeyProtection.of(null, legacyInsideSecureHardware(info))
        }
    }

    private companion object {
        const val PROVIDER = "AndroidKeyStore"

        /** Before API 31 this is the only way to ask. */
        @Suppress("DEPRECATION")
        fun legacyInsideSecureHardware(info: KeyInfo) = info.isInsideSecureHardware
    }
}
