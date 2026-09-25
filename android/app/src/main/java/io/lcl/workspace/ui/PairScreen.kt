package io.lcl.workspace.ui

import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import io.lcl.workspace.AppContainer
import io.lcl.workspace.connection.PendingPairing
import io.lcl.workspace.remote.InvalidLink
import io.lcl.workspace.remote.PairingLink
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * Pair with a PC from the one-time QR code it shows. Scanning or pasting only
 * fills the form in. Pair asks the PC to trust this device; the screen then
 * shows the verification code and waits until the person approves the
 * request on the PC — or denies it, it expires, or they cancel here.
 */
@Composable
fun PairScreen(container: AppContainer, onPaired: () -> Unit, onBack: () -> Unit) {
    var link by remember { mutableStateOf("") }
    var deviceName by remember { mutableStateOf(Build.MODEL ?: "Android device") }
    var working by remember { mutableStateOf(false) }
    var waiting by remember { mutableStateOf<PendingPairing?>(null) }
    var attempt by remember { mutableStateOf<Job?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    fun pair(text: String) {
        problem = null
        val parsed = try {
            PairingLink.parse(text)
        } catch (e: InvalidLink) {
            problem = e.message
            return
        }
        working = true
        attempt = scope.launch {
            try {
                val result = container.connection.pair(parsed, deviceName.ifBlank { "Android device" }) { waiting = it }
                result.onSuccess { onPaired() }.onFailure { problem = it.message ?: "Pairing failed." }
            } finally {
                working = false
                waiting = null
                attempt = null
            }
        }
    }

    // Leaving the screen cancels the attempt too (the scope ends with it),
    // and a cancelled attempt deletes its key and saves nothing.
    fun cancel() {
        attempt?.cancel()
        problem = "Pairing cancelled. Nothing was paired."
    }

    // A scanned code, like pasted text, only fills the form in: no key is
    // made and no connection is tried until Pair is pressed.
    val scanner = rememberLauncherForActivityResult(ScanContract()) { result ->
        result.contents?.let {
            problem = null
            link = it.trim()
        }
    }
    // Only the scanner and the person's own paste fill the form in; no other
    // app can hand it a pairing code (see LclRoot). Asking a PC for trust is
    // always this person's decision, made here, with the PC's name and
    // fingerprint in front of them; granting it is the PC's, made there.
    val preview = remember(link) { runCatching { PairingLink.parse(link) }.getOrNull() }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Pair a PC", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        val pending = waiting
        if (pending != null) {
            Card(Modifier.fillMaxWidth().testTag("pair_waiting")) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Waiting for approval on ${pending.pcName.ifBlank { "the PC" }}", fontWeight = FontWeight.SemiBold)
                    Text("Verification code", style = MaterialTheme.typography.bodySmall)
                    Text(
                        pending.verification,
                        style = MaterialTheme.typography.headlineMedium,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.testTag("pair_verification"),
                    )
                    Text(
                        "On the PC, approve the pending device only if this code matches: run `lcl-remote pending`, " +
                            "or open Settings → Android devices in the LCL workspace.",
                    )
                    val minutes = (pending.expires - System.currentTimeMillis() / 1000) / 60
                    Text(
                        "Nothing is trusted until then. The request ends with the code, in about ${maxOf(minutes, 0) + 1} minute(s).",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                OutlinedButton(onClick = { cancel() }, modifier = Modifier.testTag("pair_cancel")) { Text("Cancel") }
                CircularProgressIndicator(Modifier.padding(start = 8.dp))
            }
        } else {
            Text(
                "On the PC, run `lcl-remote pair`, or open Settings → Android devices in the LCL workspace, " +
                    "and scan the QR code. Scanning trusts nothing: after Pair, you approve this device on the PC. " +
                    "Once paired, it reconnects by itself until you Forget the PC here or the PC revokes it.",
            )
            OutlinedTextField(
                value = deviceName,
                onValueChange = { deviceName = it.take(64) },
                label = { Text("This device's name, as the PC will list it") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Button(
                onClick = {
                    scanner.launch(
                        ScanOptions()
                            .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                            .setPrompt("Scan the QR code the PC shows")
                            .setBeepEnabled(false)
                            .setOrientationLocked(false),
                    )
                },
                enabled = !working,
                modifier = Modifier.fillMaxWidth().testTag("scan"),
            ) { Text("Scan QR code") }
            Text("Or paste the pairing text the PC printed:", style = MaterialTheme.typography.bodySmall)
            OutlinedTextField(
                value = link,
                onValueChange = { link = it.trim() },
                label = { Text("LCLPAIR|v=2&…") },
                modifier = Modifier.fillMaxWidth().testTag("pair_link"),
                maxLines = 4,
            )
            preview?.let { pc ->
                Card(Modifier.fillMaxWidth().testTag("pair_preview")) {
                    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text("Pair with ${pc.pcName.ifBlank { "this PC" }}?", fontWeight = FontWeight.SemiBold)
                        Text(
                            "Fingerprint " + pc.fingerprint.chunked(4).take(8).joinToString(" ") + " …",
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text("Tries: " + pc.addresses.joinToString(", "), style = MaterialTheme.typography.bodySmall)
                        val minutes = (pc.expires - System.currentTimeMillis() / 1000) / 60
                        Text(
                            if (minutes < 0) "This code has expired." else "The code works for about ${minutes + 1} more minute(s).",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text(
                            "Pair only with a PC you control: the PC's screen shows the same fingerprint.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Button(onClick = { pair(link) }, enabled = !working && link.isNotBlank(), modifier = Modifier.testTag("pair_button")) {
                    Text("Pair")
                }
                OutlinedButton(onClick = onBack, enabled = !working) { Text("Back") }
                if (working) CircularProgressIndicator(Modifier.padding(start = 8.dp))
            }
        }
        problem?.let {
            Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("pair_problem"))
        }
    }
}
