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
import io.lcl.workspace.remote.InvalidLink
import io.lcl.workspace.remote.PairingLink
import kotlinx.coroutines.launch

/** Pair with a PC from the one-time QR code it shows. */
@Composable
fun PairScreen(container: AppContainer, initialLink: String?, onPaired: () -> Unit, onBack: () -> Unit) {
    var link by remember { mutableStateOf(initialLink ?: "") }
    var deviceName by remember { mutableStateOf(Build.MODEL ?: "Android device") }
    var working by remember { mutableStateOf(false) }
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
        scope.launch {
            val result = container.connection.pair(parsed, deviceName.ifBlank { "Android device" })
            working = false
            result.onSuccess { onPaired() }.onFailure { problem = it.message ?: "Pairing failed." }
        }
    }

    val scanner = rememberLauncherForActivityResult(ScanContract()) { result ->
        result.contents?.let {
            link = it
            pair(it)
        }
    }
    // A link from outside the app (a web page, another app, the system camera)
    // only fills the form in. Trusting a PC is always this person's decision,
    // made here, with the PC's name and fingerprint in front of them.
    val preview = remember(link) { runCatching { PairingLink.parse(link) }.getOrNull() }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Pair a PC", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Text(
            "On the PC, run `lcl-remote pair`, or open Settings → Android devices in the LCL workspace, " +
                "and scan the QR code. A code works once and for a few minutes. After pairing, this device " +
                "reconnects by itself — after a restart, a network change or a day offline — until you " +
                "Forget the PC here or the PC revokes this device.",
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
        Text("Or paste the pairing link the PC printed:", style = MaterialTheme.typography.bodySmall)
        OutlinedTextField(
            value = link,
            onValueChange = { link = it.trim() },
            label = { Text("lclpair://pair?…") },
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
                        if (minutes < 0) "This code has expired." else "The code works once, for about ${minutes + 1} more minute(s).",
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
        problem?.let {
            Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("pair_problem"))
        }
    }
}
