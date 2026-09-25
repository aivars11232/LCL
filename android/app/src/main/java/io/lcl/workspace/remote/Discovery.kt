package io.lcl.workspace.remote

import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.SocketTimeoutException

/**
 * Finding a paired PC on the local network after its address changed.
 *
 * A broadcast asks for one PC by id; the PC answers with its port. The answer
 * is only a hint of where to try: whoever answers still has to present the
 * pinned certificate before this device says anything to it.
 */
object Discovery {
    const val PORT = 47301

    fun find(pcId: String, waitMs: Long = 1_200): List<String> {
        val found = linkedSetOf<String>()
        runCatching {
            DatagramSocket().use { socket ->
                socket.broadcast = true
                socket.soTimeout = 200
                val ask = "LCL-DISCOVER 1 $pcId".toByteArray(Charsets.UTF_8)
                socket.send(DatagramPacket(ask, ask.size, InetAddress.getByName("255.255.255.255"), PORT))
                val deadline = System.currentTimeMillis() + waitMs
                val buffer = ByteArray(256)
                while (System.currentTimeMillis() < deadline) {
                    val packet = DatagramPacket(buffer, buffer.size)
                    try {
                        socket.receive(packet)
                    } catch (_: SocketTimeoutException) {
                        continue
                    }
                    val reply = String(packet.data, 0, packet.length, Charsets.UTF_8).split(' ')
                    if (reply.size == 4 && reply[0] == "LCL-HERE" && reply[1] == "1" && reply[2] == pcId) {
                        val port = reply[3].toIntOrNull()?.takeIf { it in 1..65535 } ?: continue
                        val host = packet.address.hostAddress ?: continue
                        found += if (host.contains(':')) "[$host]:$port" else "$host:$port"
                    }
                }
            }
        }
        return found.toList()
    }
}
