package io.lcl.workspace

import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.connection.PAIRING_POLL_MS
import io.lcl.workspace.connection.PendingPairing
import io.lcl.workspace.data.MemoryStore
import io.lcl.workspace.data.PcStore
import io.lcl.workspace.remote.PairingVerification
import io.lcl.workspace.remote.Transport
import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.str
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ConnectionManagerTest {
    private val backing = MemoryStore()
    private val store = PcStore(backing)
    private val identities = FakeIdentities()
    private val network = FakeNetwork()
    private var clock = 1_000_000L
    private val home = "192.168.1.20:47300"
    private val pc = FakePc(reachable = setOf(home))
    private var discovered: List<String> = emptyList()

    private fun TestScope.manager(transport: Transport = pc, scope: CoroutineScope = backgroundScope): ConnectionManager =
        ConnectionManager(store, identities, transport, network, scope, discover = { discovered }, now = { clock })
            .also { runCurrent() }

    private suspend fun TestScope.paired(transport: Transport = pc): ConnectionManager {
        val manager = manager(transport)
        manager.pair(pc.link(clock), "Pixel").getOrThrow()
        runCurrent()
        assertTrue(manager.state.value is ConnectionState.Connected)
        return manager
    }

    /** The connection drops and the loop's first wait passes. */
    private fun TestScope.dropAndWait(seconds: Int = 1) {
        pc.sessions.last().drop()
        runCurrent()
        advanceTimeBy(seconds * 1000L + 1)
        runCurrent()
    }

    private val ConnectionManager.connected get() = state.value as ConnectionState.Connected

    // ------------------------------------------------------------------ pairing

    @Test
    fun pairing_trusts_the_pc_by_its_fingerprint_and_keeps_no_code() = runTest {
        val manager = manager()
        val link = pc.link(clock)
        val record = manager.pair(link, "Pixel").getOrThrow()
        runCurrent()
        assertEquals(pc.fingerprint, record.fingerprint)
        assertEquals("dev1", record.deviceId)
        assertTrue(identities.keys.containsKey(record.keyAlias))
        assertEquals("pc1", manager.connected.pc.pcId)
        // It asked, the PC approved, it asked again: both times as the
        // current pairing flow.
        assertEquals(2, pc.hellos.size)
        for (hello in pc.hellos) {
            assertEquals("pair", hello.str("intent"))
            assertEquals(2L, hello.long("pairing_version"))
            assertEquals("Pixel", hello.str("name"))
        }
        // The one-time code is spent, and nothing on the device keeps it.
        assertNull(pc.code)
        assertFalse(backing.get("pcs")!!.contains(link.code))
        assertEquals("pc1", store.activeId())
    }

    @Test
    fun pairing_waits_for_the_pc_and_saves_nothing_until_approved() = runTest {
        val manager = manager()
        pc.decision = FakePc.Decision.WAIT
        val link = pc.link(clock)
        val shown = mutableListOf<PendingPairing>()
        val result = async { manager.pair(link, "Pixel") { shown += it } }
        runCurrent()
        // Pending: the verification code is this device's own, and nothing
        // is saved, active or connected.
        val key = identities.keys.keys.single()
        assertEquals(PairingVerification.of(pc.fingerprint, link.code, testIdentity().fingerprint), shown.last().verification)
        assertEquals("Test PC", shown.last().pcName)
        assertTrue(store.all().isEmpty())
        assertNull(store.activeId())
        assertEquals(ConnectionState.NoPc, manager.state.value)
        assertFalse(result.isCompleted)
        // It asks again every two seconds, never sooner.
        val asked = pc.attempts
        advanceTimeBy(PAIRING_POLL_MS - 1)
        runCurrent()
        assertEquals(asked, pc.attempts)
        advanceTimeBy(2)
        runCurrent()
        assertEquals(asked + 1, pc.attempts)
        assertTrue("a PC was saved while the request was pending", store.all().isEmpty())
        // Approved on the PC: saved, active and connected, with the same key.
        pc.approve(testIdentity().fingerprint)
        advanceTimeBy(PAIRING_POLL_MS)
        runCurrent()
        val record = result.await().getOrThrow()
        assertEquals(key, record.keyAlias)
        assertEquals(record, store.get("pc1"))
        assertEquals("pc1", store.activeId())
        assertTrue(manager.state.value is ConnectionState.Connected)
    }

    @Test
    fun a_denied_request_deletes_its_key_and_saves_nothing() = runTest {
        val manager = manager()
        pc.decision = FakePc.Decision.DENY
        val failure = manager.pair(pc.link(clock), "Pixel").exceptionOrNull()!!
        assertTrue(failure.message, failure.message!!.contains("denied"))
        assertTrue(identities.keys.isEmpty())
        assertEquals(1, identities.deleted.size)
        assertTrue(store.all().isEmpty())
        assertEquals(ConnectionState.NoPc, manager.state.value)
    }

    @Test
    fun a_request_nobody_approves_expires_and_deletes_its_key() = runTest {
        val manager = manager()
        pc.decision = FakePc.Decision.WAIT
        val result = async { manager.pair(pc.link(clock), "Pixel") }
        runCurrent()
        assertEquals(1, identities.keys.size)
        clock += 300
        advanceTimeBy(PAIRING_POLL_MS + 1)
        runCurrent()
        val failure = result.await().exceptionOrNull()!!
        assertTrue(failure.message, failure.message!!.contains("expired"))
        assertTrue(identities.keys.isEmpty())
        assertTrue(store.all().isEmpty())
    }

    @Test
    fun cancelling_a_waiting_request_deletes_its_key_and_stops_asking() = runTest {
        val manager = manager()
        pc.decision = FakePc.Decision.WAIT
        val attempt = launch { manager.pair(pc.link(clock), "Pixel") }
        runCurrent()
        assertEquals(1, identities.keys.size)
        attempt.cancel()
        runCurrent()
        assertTrue(identities.keys.isEmpty())
        assertTrue(store.all().isEmpty())
        val asked = pc.attempts
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals("a cancelled request kept asking", asked, pc.attempts)
    }

    @Test
    fun a_waiting_request_outlasts_a_moment_without_the_pc() = runTest {
        val manager = manager()
        pc.decision = FakePc.Decision.WAIT
        val result = async { manager.pair(pc.link(clock), "Pixel") }
        runCurrent()
        pc.reachable = emptySet()
        advanceTimeBy(PAIRING_POLL_MS * 3)
        runCurrent()
        assertFalse(result.isCompleted)
        assertEquals(1, identities.keys.size)
        pc.reachable = setOf(home)
        pc.approve(testIdentity().fingerprint)
        advanceTimeBy(PAIRING_POLL_MS + 1)
        runCurrent()
        assertTrue(result.await().isSuccess)
    }

    @Test
    fun a_pc_showing_another_verification_code_is_not_paired() = runTest {
        val manager = manager()
        pc.wrongVerification = true
        val failure = manager.pair(pc.link(clock), "Pixel").exceptionOrNull()!!
        assertTrue(failure.message, failure.message!!.contains("verification code does not match"))
        assertTrue(identities.keys.isEmpty())
        assertTrue(store.all().isEmpty())
    }

    @Test
    fun a_key_left_by_an_unfinished_pairing_is_deleted_at_the_next_start() = runTest {
        paired()
        val used = store.get("pc1")!!.keyAlias
        identities.keys["lcl-pc-pc1-left-behind"] = testIdentity()
        identities.keys["not-this-app-s"] = testIdentity()
        manager().start()
        runCurrent()
        assertEquals(setOf(used, "not-this-app-s"), identities.keys.keys)
    }

    @Test
    fun a_pc_paired_before_approval_existed_reconnects_without_a_qr_code() = runTest {
        // What an earlier version of the app saved, and the key it made.
        backing.put(
            "pcs",
            """[{"pc_id":"pc1","name":"Test PC","fingerprint":"${pc.fingerprint}","addresses":["$home"],""" +
                """"device_id":"dev9","key_alias":"lcl-pc-pc1-0b1c","paired_at":999000,"last_connected":999100,"last_address":"$home"}]""",
        )
        backing.put("active_pc", "pc1")
        identities.keys["lcl-pc-pc1-0b1c"] = testIdentity()
        pc.devices[testIdentity().fingerprint] = "dev9"
        val manager = manager()
        manager.start()
        runCurrent()
        assertEquals("pc1", manager.connected.pc.pcId)
        val hello = pc.hellos.single()
        assertEquals("connect", hello.str("intent"))
        assertEquals("dev9", hello.str("device"))
        assertTrue(identities.keys.containsKey("lcl-pc-pc1-0b1c"))
    }

    @Test
    fun a_used_or_expired_code_pairs_nothing_and_leaves_no_key() = runTest {
        val manager = manager()
        val used = pc.link(clock).also { pc.code = null }
        val refused = manager.pair(used, "Pixel")
        assertTrue(refused.exceptionOrNull()!!.message!!.contains("already used"))
        val expired = pc.link(clock).also { clock += 301 }
        val attempts = pc.attempts
        assertTrue(manager.pair(expired, "Pixel").exceptionOrNull()!!.message!!.contains("expired"))
        assertEquals("an expired code was sent to the PC", attempts, pc.attempts)
        assertTrue(store.all().isEmpty())
        assertTrue(identities.keys.isEmpty())
        assertEquals(ConnectionState.NoPc, manager.state.value)
    }

    @Test
    fun a_machine_without_the_pinned_certificate_is_not_paired() = runTest {
        val manager = manager()
        val link = pc.link(clock).copy(fingerprint = "d".repeat(64))
        val failure = manager.pair(link, "Pixel").exceptionOrNull()!!
        assertTrue(failure.message, failure.message!!.contains("not the PC in this QR code"))
        assertTrue(pc.hellos.isEmpty())
        assertTrue(store.all().isEmpty())
        assertTrue(identities.keys.isEmpty())
    }

    @Test
    fun pairing_again_replaces_the_key_only_when_it_succeeds() = runTest {
        val manager = paired()
        val first = store.get("pc1")!!.keyAlias
        val used = pc.link(clock).also { pc.code = null }
        assertTrue(manager.pair(used, "Pixel").isFailure)
        // Denied, or never approved: the working pairing is untouched.
        pc.decision = FakePc.Decision.DENY
        assertTrue(manager.pair(pc.link(clock), "Pixel").isFailure)
        pc.decision = FakePc.Decision.WAIT
        val waiting = launch { manager.pair(pc.link(clock), "Pixel") }
        runCurrent()
        waiting.cancel()
        runCurrent()
        assertEquals(first, store.get("pc1")!!.keyAlias)
        assertEquals(setOf(first), identities.keys.keys)
        assertTrue("a failed pairing broke the working one", manager.state.value is ConnectionState.Connected)
        pc.decision = FakePc.Decision.APPROVE
        val second = manager.pair(pc.link(clock), "Pixel").getOrThrow().keyAlias
        runCurrent()
        assertNotEquals(first, second)
        assertTrue(first in identities.deleted)
        assertEquals(setOf(second), identities.keys.keys)
    }

    // ------------------------------------------------------------- persistence

    @Test
    fun a_restarted_app_reconnects_without_a_qr_code() = runTest {
        val process = CoroutineScope(backgroundScope.coroutineContext + Job(backgroundScope.coroutineContext[Job]))
        manager(scope = process).pair(pc.link(clock), "Pixel").getOrThrow()
        runCurrent()
        process.cancel() // the app is closed, or the phone restarts
        pc.sessions.last().drop()
        runCurrent()
        val again = manager()
        again.start()
        runCurrent()
        assertEquals("pc1", again.connected.pc.pcId)
        val hello = pc.hellos.last()
        assertEquals("connect", hello.str("intent"))
        assertEquals("dev1", hello.str("device"))
        assertNull("a reconnect sent a pairing code", hello.str("code"))
    }

    @Test
    fun a_dropped_connection_comes_back_by_itself() = runTest {
        val manager = paired()
        pc.sessions.last().drop()
        runCurrent()
        val waiting = manager.state.value as ConnectionState.Reconnecting
        assertEquals(1, waiting.retryInSeconds)
        advanceTimeBy(1_001)
        runCurrent()
        assertEquals(2, pc.sessions.size)
        assertEquals(pc.sessions.last(), manager.connected.session)
    }

    @Test
    fun an_unreachable_pc_is_retried_with_growing_waits() = runTest {
        val manager = paired()
        pc.reachable = emptySet()
        pc.sessions.last().drop()
        runCurrent()
        val waits = mutableListOf<Int>()
        repeat(7) {
            val state = manager.state.value as ConnectionState.Reconnecting
            waits += state.retryInSeconds
            advanceTimeBy(state.retryInSeconds * 1000L + 1)
            runCurrent()
        }
        assertEquals(listOf(1, 2, 4, 8, 15, 30, 30), waits)
        pc.reachable = setOf(home)
        advanceTimeBy(30_001)
        runCurrent()
        assertTrue(manager.state.value is ConnectionState.Connected)
    }

    @Test
    fun reconnect_now_skips_the_wait() = runTest {
        val manager = paired()
        pc.reachable = emptySet()
        repeat(3) { dropAndWait() }
        pc.reachable = setOf(home)
        manager.reconnectNow()
        runCurrent()
        assertTrue(manager.state.value is ConnectionState.Connected)
    }

    @Test
    fun without_a_network_it_waits_and_resumes_when_one_appears() = runTest {
        val manager = paired()
        network.available.value = false
        dropAndWait()
        assertTrue(manager.state.value is ConnectionState.Offline)
        val attempts = pc.attempts
        advanceTimeBy(600_000)
        runCurrent()
        assertEquals("tried to connect with no network", attempts, pc.attempts)
        network.available.value = true
        runCurrent()
        assertTrue(manager.state.value is ConnectionState.Connected)
    }

    @Test
    fun a_network_change_reconnects_at_once() = runTest {
        val manager = paired()
        val before = pc.sessions.last()
        network.changes.emit(Unit)
        runCurrent()
        assertTrue(before.closed.isCompleted)
        assertEquals(2, pc.sessions.size)
        assertEquals(pc.sessions.last(), manager.connected.session)
    }

    @Test
    fun a_pc_on_a_new_address_is_found_and_remembered() = runTest {
        val manager = paired()
        val moved = "10.0.0.7:47300"
        pc.reachable = setOf(moved)
        discovered = listOf(moved)
        dropAndWait()
        assertEquals(moved, manager.connected.address)
        val record = store.get("pc1")!!
        assertEquals(moved, record.lastAddress)
        assertEquals(moved, record.addresses.first())
        assertTrue(home in record.addresses)
    }

    @Test
    fun a_different_machine_at_the_old_address_is_refused() = runTest {
        val lan = Lan(pc)
        val manager = paired(lan)
        val impostor = FakePc(pcId = "pc1", name = "Test PC", fingerprint = "c".repeat(64), reachable = setOf(home))
        pc.reachable = emptySet()
        lan.machines = listOf(impostor, pc)
        dropAndWait()
        val waiting = manager.state.value as ConnectionState.Reconnecting
        assertTrue(waiting.reason, waiting.reason.contains("not the PC this device paired with"))
        assertTrue("the impostor was told who this device is", impostor.hellos.isEmpty())
        assertEquals(1, impostor.attempts)
    }

    // ------------------------------------------- disconnect, forget, revoke

    @Test
    fun disconnect_ends_the_connection_but_keeps_the_pairing() = runTest {
        val manager = paired()
        val pairing = pc.attempts
        val session = pc.sessions.last()
        manager.disconnect()
        runCurrent()
        assertTrue(session.closed.isCompleted)
        assertTrue(manager.state.value is ConnectionState.Disconnected)
        advanceTimeBy(600_000)
        runCurrent()
        assertEquals("reconnected after Disconnect", pairing, pc.attempts)
        assertNotNull(store.get("pc1"))
        assertEquals(1, identities.keys.size)
        manager.reconnectNow()
        runCurrent()
        assertTrue(manager.state.value is ConnectionState.Connected)
        assertEquals("connect", pc.hellos.last().str("intent"))
    }

    @Test
    fun disconnect_during_a_connection_attempt_stays_disconnected() = runTest {
        val manager = paired()
        pc.hold = CompletableDeferred()
        dropAndWait() // the loop is now inside a connection attempt
        assertTrue(manager.state.value is ConnectionState.Connecting)
        manager.disconnect()
        runCurrent()
        pc.hold!!.complete(Unit)
        advanceTimeBy(600_000)
        runCurrent()
        assertTrue(manager.state.value.toString(), manager.state.value is ConnectionState.Disconnected)
        assertEquals("a connection opened after Disconnect", 1, pc.sessions.size)
    }

    @Test
    fun forget_deletes_the_key_and_the_record() = runTest {
        val manager = paired()
        val pairing = pc.attempts
        val alias = store.get("pc1")!!.keyAlias
        manager.forget("pc1")
        runCurrent()
        assertEquals(ConnectionState.NoPc, manager.state.value)
        assertNull(store.get("pc1"))
        assertNull(store.activeId())
        assertTrue(alias in identities.deleted)
        assertTrue(identities.keys.isEmpty())
        // The PC was listening, so it was asked to stop trusting this device.
        assertEquals(listOf("unpair"), pc.sessions.last().requests.map { it.first })
        assertTrue(pc.sessions.last().closed.isCompleted)
        manager().start() // the next app start has nothing to go back to
        runCurrent()
        assertEquals(pairing, pc.attempts)
    }

    @Test
    fun forget_works_without_the_pc() = runTest {
        val manager = paired()
        pc.reachable = emptySet()
        dropAndWait()
        assertTrue(manager.state.value is ConnectionState.Reconnecting)
        manager.forget("pc1")
        runCurrent()
        assertEquals(ConnectionState.NoPc, manager.state.value)
        assertTrue(identities.keys.isEmpty())
        assertNull(store.get("pc1"))
        val attempts = pc.attempts
        advanceTimeBy(600_000)
        runCurrent()
        assertEquals("a forgotten PC was tried again", attempts, pc.attempts)
    }

    @Test
    fun a_revoked_device_stops_and_does_not_retry() = runTest {
        val manager = paired()
        pc.revoked += testIdentity().fingerprint
        dropAndWait()
        val revoked = manager.state.value as ConnectionState.Revoked
        assertTrue(revoked.message.contains("revoked"))
        val attempts = pc.attempts
        advanceTimeBy(600_000)
        runCurrent()
        assertEquals(attempts, pc.attempts)
    }

    @Test
    fun revocation_during_a_session_ends_it() = runTest {
        val manager = paired()
        val pairing = pc.attempts
        val session = pc.sessions.last()
        session.emit(buildJsonObject { put("type", "event"); put("event", "revoked") })
        runCurrent()
        assertTrue(manager.state.value is ConnectionState.Revoked)
        assertTrue(session.closed.isCompleted)
        advanceTimeBy(600_000)
        runCurrent()
        assertEquals(pairing, pc.attempts)
    }

    @Test
    fun a_pc_that_forgot_this_device_asks_for_a_new_pairing() = runTest {
        val manager = paired()
        pc.devices.clear()
        dropAndWait()
        assertTrue(manager.state.value is ConnectionState.NotPaired)
    }

    @Test
    fun a_missing_key_is_reported_instead_of_retried() = runTest {
        val manager = paired()
        identities.keys.clear()
        dropAndWait()
        val state = manager.state.value as ConnectionState.NotPaired
        assertTrue(state.message.contains("key"))
    }

    // ----------------------------------------------------------- several PCs

    @Test
    fun several_pcs_are_kept_apart() = runTest {
        val laptop = FakePc(pcId = "pc2", name = "Laptop", fingerprint = "b".repeat(64), reachable = setOf("192.168.1.30:47300"))
        val manager = manager(Lan(pc, laptop))
        manager.pair(pc.link(clock), "Pixel").getOrThrow()
        runCurrent()
        manager.pair(laptop.link(clock), "Pixel").getOrThrow()
        runCurrent()
        assertEquals(setOf("pc1", "pc2"), manager.pcs().map { it.pcId }.toSet())
        assertEquals("pc2", manager.connected.pc.pcId)
        assertTrue("switching PCs left the other connection open", pc.sessions.last().closed.isCompleted)
        manager.connect("pc1")
        runCurrent()
        assertEquals("pc1", manager.connected.pc.pcId)
        assertEquals(pc.sessions.last(), manager.connected.session)
        manager.forget("pc2")
        runCurrent()
        assertEquals(listOf("pc1"), manager.pcs().map { it.pcId })
        assertEquals("forgetting another PC touched this connection", "pc1", manager.connected.pc.pcId)
        assertEquals(setOf(store.get("pc1")!!.keyAlias), identities.keys.keys)
    }
}
