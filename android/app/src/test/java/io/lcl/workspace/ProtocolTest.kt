package io.lcl.workspace

import io.lcl.workspace.remote.Frames
import io.lcl.workspace.remote.InvalidLink
import io.lcl.workspace.remote.PairingLink
import io.lcl.workspace.remote.PinnedPc
import io.lcl.workspace.remote.Protocol
import io.lcl.workspace.remote.sha256Hex
import io.lcl.workspace.remote.splitAddress
import io.lcl.workspace.remote.str
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.security.cert.CertificateException
import java.util.Base64

class PairingLinkTest {
    private val code = Base64.getUrlEncoder().withoutPadding().encodeToString(ByteArray(32) { 7 })
    private val good = "lclpair://pair?v=1&pc=0123456789abcdef0123456789abcdef&n=Aivars%27%20PC&fp=${"a".repeat(64)}" +
        "&a=192.168.1.20:47300&a=%5Bfe80::1%5D:47300&c=$code&e=1790000000"

    @Test
    fun a_link_the_pc_writes_is_read_exactly() {
        val link = PairingLink.parse(good)
        assertEquals(1, link.version)
        assertEquals("Aivars' PC", link.pcName)
        assertEquals("a".repeat(64), link.fingerprint)
        assertEquals(listOf("192.168.1.20:47300", "[fe80::1]:47300"), link.addresses)
        assertEquals(code, link.code)
        assertTrue(link.isExpired(1_790_000_000))
        assertFalse(link.isExpired(1_789_999_999))
    }

    @Test
    fun malformed_links_are_refused_with_a_reason() {
        for (bad in listOf(
            "https://example.com/pair?v=1",
            good.replace("v=1", "v=2"),
            good.replace("a".repeat(64), "A".repeat(64)),
            good.replace("a".repeat(64), "a".repeat(63)),
            good.replace(code, "short"),
            good.replace("&a=192.168.1.20:47300&a=%5Bfe80::1%5D:47300", ""),
            "$good&c=again",
            good.replace("e=1790000000", "e=soon"),
            good.replace("n=Aivars%27%20PC", "n=bad%2"),
        )) {
            assertThrows(bad, InvalidLink::class.java) { PairingLink.parse(bad) }
        }
    }
}

class FramesTest {
    @Test
    fun frames_round_trip_unicode_and_empty_messages() {
        val bytes = ByteArrayOutputStream()
        val out = DataOutputStream(bytes)
        for (m in listOf("{\"a\":1}", "", "{\"b\":\"ü → ✓\"}")) Frames.write(out, m)
        val input = DataInputStream(ByteArrayInputStream(bytes.toByteArray()))
        assertEquals("{\"a\":1}", Frames.read(input))
        assertEquals("", Frames.read(input))
        assertEquals("{\"b\":\"ü → ✓\"}", Frames.read(input))
    }

    @Test
    fun an_oversized_or_invalid_frame_is_refused() {
        val huge = DataInputStream(ByteArrayInputStream(byteArrayOf(0x7f, 0, 0, 0)))
        assertThrows(IOException::class.java) { Frames.read(huge) }
        val invalid = DataInputStream(ByteArrayInputStream(byteArrayOf(0, 0, 0, 2, 0xff.toByte(), 0xfe.toByte())))
        assertThrows(IOException::class.java) { Frames.read(invalid) }
    }

    @Test
    fun messages_carry_the_protocol_version() {
        val hello = Protocol.helloPair("code", "Pixel")
        assertEquals("lcl.remote/1", hello.str("protocol"))
        assertEquals("pair", hello.str("intent"))
        val connect = Protocol.helloConnect("dev1")
        assertEquals("connect", connect.str("intent"))
        assertEquals("dev1", connect.str("device"))
    }

    @Test
    fun addresses_split_with_ipv6_brackets() {
        assertEquals("192.168.1.2" to 47300, splitAddress("192.168.1.2:47300"))
        assertEquals("fe80::1" to 47300, splitAddress("[fe80::1]:47300"))
        assertThrows(IllegalArgumentException::class.java) { splitAddress("nohost") }
        assertThrows(IllegalArgumentException::class.java) { splitAddress("h:99999") }
    }
}

class PinningTest {
    @Test
    fun only_the_pinned_certificate_is_trusted() {
        val identity = testIdentity()
        val pinned = PinnedPc(sha256Hex(identity.certificate.encoded))
        pinned.checkServerTrusted(arrayOf(identity.certificate), "EC")
        val other = PinnedPc("0".repeat(64))
        assertThrows(CertificateException::class.java) { other.checkServerTrusted(arrayOf(identity.certificate), "EC") }
        assertThrows(CertificateException::class.java) { pinned.checkServerTrusted(emptyArray(), "EC") }
        assertThrows(CertificateException::class.java) { pinned.checkClientTrusted(arrayOf(identity.certificate), "EC") }
    }
}
