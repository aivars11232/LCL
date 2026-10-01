package io.lcl.workspace.update

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.content.pm.SigningInfo
import android.os.Build
import android.provider.Settings
import androidx.core.content.IntentCompat
import androidx.core.net.toUri
import io.lcl.workspace.LclApplication
import java.io.File

/** The SHA-256 of each current signing certificate. */
private fun signers(info: SigningInfo?): Set<String> =
    info?.apkContentsSigners?.map { sha256(it.toByteArray()) }?.toSet() ?: emptySet()

class AndroidPackageFacts(private val context: Context) : PackageFacts {
    override fun installed(): InstalledApp {
        val info =
            context.packageManager.getPackageInfo(
                context.packageName,
                PackageManager.GET_SIGNING_CERTIFICATES,
            )
        return InstalledApp(
            info.versionName ?: "",
            info.longVersionCode,
            Build.VERSION.SDK_INT,
            signers(info.signingInfo),
        )
    }

    override fun archive(file: File): ApkFacts? =
        context.packageManager
            .getPackageArchiveInfo(file.path, PackageManager.GET_SIGNING_CERTIFICATES)
            ?.let {
                ApkFacts(
                    it.packageName,
                    it.versionName ?: "",
                    it.longVersionCode,
                    signers(it.signingInfo),
                )
            }
}

/**
 * Android's own installer, as any app may use it: a session the person confirms when Android asks.
 * Nothing privileged, nothing silent.
 */
class PackageInstallerApk(private val context: Context) : ApkInstaller {
    override fun canInstall() = context.packageManager.canRequestPackageInstalls()

    override fun askPermission() {
        context.startActivity(
            Intent(
                    Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    "package:${context.packageName}".toUri(),
                )
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }

    override fun install(file: File) {
        val installer = context.packageManager.packageInstaller
        val params =
            PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        params.setAppPackageName(context.packageName)
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            session.openWrite("lcl.apk", 0, file.length()).use { out ->
                file.inputStream().use { it.copyTo(out) }
                session.fsync(out)
            }
            val status =
                Intent(context, UpdateStatusReceiver::class.java).setPackage(context.packageName)
            // Mutable: the installer adds its status to it.
            val pending =
                PendingIntent.getBroadcast(
                    context,
                    id,
                    status,
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
                )
            session.commit(pending.intentSender)
        }
    }
}

/** The installer's answer, which only the installer can send (not exported). */
class UpdateStatusReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status =
            intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
        val confirm =
            IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)
        val updates = (context.applicationContext as LclApplication).container.updates
        updates.onInstallStatus(
            status,
            message,
            confirm?.let { { context.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } },
        )
    }
}
