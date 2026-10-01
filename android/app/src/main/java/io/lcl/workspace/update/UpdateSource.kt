package io.lcl.workspace.update

import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.HttpURLConnection
import java.net.URL

/** Where releases are found, and their assets fetched. */
interface ReleaseSource {
    /** The latest published stable release, or null when there is none. */
    fun latest(): Release?

    /**
     * Asset [name] of [release], written to [into] as it arrives, at most [limit] bytes; [progress]
     * sees (done, total).
     */
    fun fetch(
        release: Release,
        name: String,
        limit: Long,
        into: OutputStream,
        progress: (Long, Long) -> Unit = { _, _ -> },
    )
}

/** A small asset — a manifest, a signature — in memory. */
fun ReleaseSource.fetchBytes(release: Release, name: String, limit: Long): ByteArray =
    ByteArrayOutputStream().also { fetch(release, name, limit, it) }.toByteArray()

/**
 * Copy [input] to [into], one buffer at a time: never more than [limit] bytes, refused as soon as
 * it would pass [limit] or as soon as the server announces more ([length], -1 when it does not
 * say), and a download that stops early or comes up short of [length] is an error. The number of
 * bytes copied.
 */
fun copyBounded(
    input: InputStream,
    into: OutputStream,
    length: Long,
    limit: Long,
    progress: (Long, Long) -> Unit = { _, _ -> },
): Long {
    if (length > limit)
        throw UpdateRefused("invalid", "$length bytes is more than the $limit expected")
    val buffer = ByteArray(64 * 1024)
    var done = 0L
    while (true) {
        val n =
            try {
                input.read(buffer)
            } catch (e: IOException) {
                throw UpdateRefused("download", "the download stopped: ${e.message}")
            }
        if (n < 0) break
        if (done + n > limit) throw UpdateRefused("invalid", "more than the $limit bytes expected")
        into.write(buffer, 0, n)
        done += n
        progress(done, if (length > 0) length else limit)
    }
    if (length >= 0 && done != length) throw UpdateRefused("download", "the download was cut short")
    return done
}

/**
 * GitHub Releases of the official repository, pinned in the build. Assets are fetched by name from
 * the same release, at URLs built here, never from a URL an answer supplies. Only a debug build may
 * name a local test server ([testEndpoint], `http://10.0.2.2:<port>`); a release build has none.
 */
class GitHubReleases(testEndpoint: String = "") : ReleaseSource {
    private val api: String
    private val download: String

    init {
        if (testEndpoint.isEmpty()) {
            api = "https://api.github.com/repos/$REPOSITORY"
            download = "https://github.com/$REPOSITORY/releases/download"
        } else {
            require(
                Regex("http://(10\\.0\\.2\\.2|127\\.0\\.0\\.1):[0-9]{1,5}").matches(testEndpoint)
            ) {
                "not a local test server: $testEndpoint"
            }
            api = "$testEndpoint/api"
            download = "$testEndpoint/download"
        }
    }

    override fun latest(): Release? {
        val body = ByteArrayOutputStream()
        try {
            get("$api/releases/latest", "application/vnd.github+json", 1L shl 20, body) { _, _ -> }
        } catch (e: NotFound) {
            return null
        }
        return parseRelease(body.toString(Charsets.UTF_8.name()))
    }

    override fun fetch(
        release: Release,
        name: String,
        limit: Long,
        into: OutputStream,
        progress: (Long, Long) -> Unit,
    ) {
        if (name !in release.assets)
            throw UpdateRefused("invalid", "release ${release.tag} has no $name")
        get("$download/${release.tag}/$name", "application/octet-stream", limit, into, progress)
    }

    private class NotFound : IOException()

    private fun get(
        url: String,
        accept: String,
        limit: Long,
        into: OutputStream,
        progress: (Long, Long) -> Unit,
    ) {
        var target = URL(url)
        repeat(6) {
            val connection = target.openConnection() as HttpURLConnection
            try {
                connection.instanceFollowRedirects = false
                connection.connectTimeout = 15_000
                connection.readTimeout = 30_000
                connection.setRequestProperty("Accept", accept)
                connection.setRequestProperty("User-Agent", "lcl-android-update")
                val status =
                    try {
                        connection.responseCode
                    } catch (e: IOException) {
                        throw Offline(e.message ?: "unreachable")
                    }
                when (status) {
                    200 -> {
                        connection.inputStream.use {
                            copyBounded(it, into, connection.contentLengthLong, limit, progress)
                        }
                        return
                    }
                    301,
                    302,
                    303,
                    307,
                    308 -> {
                        val next =
                            URL(
                                target,
                                connection.getHeaderField("Location")
                                    ?: throw UpdateRefused(
                                        "invalid",
                                        "a redirect without a location",
                                    ),
                            )
                        if (next.protocol != "https" && next.protocol != target.protocol)
                            throw UpdateRefused("invalid", "a redirect away from https")
                        target = next
                    }
                    404 -> throw NotFound()
                    else ->
                        throw UpdateRefused("invalid", "the update server answered HTTP $status")
                }
            } finally {
                connection.disconnect()
            }
        }
        throw UpdateRefused("invalid", "too many redirects")
    }

    companion object {
        const val REPOSITORY = "aivars11232/LCL"
    }
}

/** The network or the host could not be reached. */
class Offline(message: String) : IOException(message)
