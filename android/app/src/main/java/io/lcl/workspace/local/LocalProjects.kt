package io.lcl.workspace.local

import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.sha256Hex
import io.lcl.workspace.remote.str
import io.lcl.workspace.workspace.LclNames
import io.lcl.workspace.workspace.LoadedFolder
import io.lcl.workspace.workspace.TreeEntry
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption

/** A project kept on this phone, in the app's private storage, until it is synced to a PC. */
data class LocalProject(val name: String) {
    val id: String get() = LocalProjects.PREFIX + name
}

/** One document of a local project, as it is on the phone's storage now. */
data class LocalDocument(val id: String, val text: String, val digest: String)

/** What a PC confirmed for one local document: the phone bytes it holds a copy of, and where. */
data class SyncRecord(val localDigest: String, val pcProject: String, val pcPath: String, val pcDigest: String, val at: Long)

/** A local operation the storage refuses, and why. */
class LocalRefused(message: String) : Exception(message)

/** A write over a document that no longer holds the bytes it was loaded at. */
class LocalConflict(val text: String, val digest: String) : Exception("the document changed since it was loaded")

/**
 * Projects on this phone: folders under the app's private files directory
 * (never cache, which Android may clear), each holding LCL documents and
 * folders exactly as a PC project would. The phone has no LCL engine: this
 * stores, lists and edits bytes, nothing more.
 *
 * Listing is lazy, one folder at a time, folders first then documents, every
 * folder shown even empty, dot entries never — the same explorer contract the
 * PC serves. Every write is atomic: the bytes go to a temporary file beside
 * the target, are flushed to disk, and replace it in one rename, so a save
 * that fails leaves the previous version whole. Paths are resolved inside
 * the project only: `..`, absolute paths and dot names are refused.
 *
 * Syncing to a PC is recorded per document in the project's `.sync.json`:
 * the digest of the local bytes a PC confirmed it holds, and where. A
 * document counts as synced only while its bytes still have that digest.
 */
class LocalProjects(private val root: File) {
    init {
        root.mkdirs()
    }

    companion object {
        const val PREFIX = "local:"
        const val MAX_CHILDREN = 4096
        private const val SYNC_FILE = ".sync.json"

        fun isLocal(projectId: String): Boolean = projectId.startsWith(PREFIX)
        fun nameOf(projectId: String): String = projectId.removePrefix(PREFIX)

        /** The SHA-256 of a document's bytes, as the PC digests a file. */
        fun digest(text: String): String = sha256Hex(text.toByteArray(Charsets.UTF_8))
    }

    // ---------------------------------------------------------------- projects

    fun projects(): List<LocalProject> = root.listFiles()
        .orEmpty()
        .filter { it.isDirectory && !it.name.startsWith(".") }
        .map { LocalProject(it.name) }
        .sortedBy { it.name }

    fun exists(name: String): Boolean = runCatching { projectDir(name).isDirectory }.getOrDefault(false)

    fun create(name: String): LocalProject {
        val dir = projectDir(name)
        if (dir.exists()) throw LocalRefused("$name already exists on this phone")
        if (!dir.mkdir()) throw LocalRefused("$name could not be created on this phone")
        return LocalProject(name)
    }

    /** Remove a whole project from the phone. Explicit, and only ever asked for by the person. */
    fun deleteProject(name: String) {
        val dir = projectDir(name)
        if (!dir.isDirectory) throw LocalRefused("$name is not on this phone")
        if (!dir.deleteRecursively()) throw LocalRefused("$name could not be removed completely")
    }

    private fun projectDir(name: String): File {
        if (name.isBlank() || name.contains('/') || name.contains('\\') || name == "." || name == ".." || name.startsWith(".")) {
            throw LocalRefused("\"$name\" is not a project name")
        }
        val dir = File(root, name)
        if (dir.canonicalFile.parentFile != root.canonicalFile) throw LocalRefused("\"$name\" is not a project name")
        return dir
    }

    /** The file for [id] inside [project], proven inside it; the project's folder for "". */
    private fun resolve(project: String, id: String): File {
        val dir = projectDir(project)
        if (id.isEmpty()) return dir
        val parts = id.split('/')
        for (part in parts) {
            if (part.isEmpty() || part == "." || part == ".." || part.startsWith(".") || part.contains('\\')) {
                throw LocalRefused("\"$id\" is not a path inside the project")
            }
        }
        val file = File(dir, parts.joinToString(File.separator))
        val canonical = file.canonicalPath
        val base = dir.canonicalPath
        if (canonical != base && !canonical.startsWith(base + File.separator)) throw LocalRefused("\"$id\" is outside the project")
        return file
    }

    private fun idOf(project: String, file: File): String =
        file.canonicalFile.relativeTo(projectDir(project).canonicalFile).path.replace(File.separatorChar, '/')

    // ----------------------------------------------------------------- explorer

    /** The direct children of one folder — every folder, then the LCL documents — and nothing below them. */
    fun children(project: String, parent: String): LoadedFolder {
        val dir = resolve(project, parent)
        if (!dir.isDirectory) throw LocalRefused("there is no such folder in the project")
        val listed = dir.listFiles() ?: throw LocalRefused("the folder is not readable")
        val join = { name: String -> if (parent.isEmpty()) name else "$parent/$name" }
        val folders = listed.filter { it.isDirectory && !it.name.startsWith(".") }.map { it.name }.sorted()
        val documents = listed.filter { it.isFile && !it.name.startsWith(".") && LclNames.isDocument(it.name) }.map { it.name }.sorted()
        val entries = folders.map { TreeEntry(join(it), true) } + documents.map { TreeEntry(join(it), false) }
        return LoadedFolder(entries.take(MAX_CHILDREN), truncated = entries.size > MAX_CHILDREN)
    }

    /** Make one empty folder, under a folder that exists. */
    fun mkdir(project: String, id: String): String {
        val trimmed = id.trim().trimEnd('/')
        val dir = resolve(project, trimmed)
        if (dir.exists()) throw LocalRefused("$trimmed already exists")
        if (dir.parentFile?.isDirectory != true) throw LocalRefused("the parent folder of $trimmed does not exist")
        if (!dir.mkdir()) throw LocalRefused("$trimmed could not be created")
        return idOf(project, dir)
    }

    /** Every document of the project, by id, deepest folders included: for syncing the whole project. */
    fun documents(project: String): List<String> {
        val out = mutableListOf<String>()
        fun walk(parent: String) {
            val folder = children(project, parent)
            for (entry in folder.entries) if (entry.directory) walk(entry.id) else out += entry.id
        }
        walk("")
        return out.sorted()
    }

    // ---------------------------------------------------------------- documents

    fun read(project: String, id: String): LocalDocument {
        val file = resolve(project, id)
        if (!file.isFile || !LclNames.isDocument(file.name)) throw LocalRefused("$id is not a document of the project")
        val bytes = file.readBytes()
        return LocalDocument(id, String(bytes, Charsets.UTF_8), sha256Hex(bytes))
    }

    /** Create a document that does not exist yet, making its folders on the way. */
    fun createFile(project: String, id: String, text: String): LocalDocument {
        val file = resolve(project, id)
        if (!LclNames.isDocument(file.name)) throw LocalRefused("$id is not an LCL document name")
        if (file.exists()) throw LocalRefused("$id already exists")
        file.parentFile?.mkdirs()
        replace(file, text)
        return LocalDocument(id, text, digest(text))
    }

    /**
     * Write [text] over [id]. With [base], only when the file still holds the
     * bytes with that digest: otherwise nothing is written and the current
     * text is reported ([LocalConflict]), as the PC refuses a stale save.
     */
    fun write(project: String, id: String, text: String, base: String?): LocalDocument {
        val file = resolve(project, id)
        if (!LclNames.isDocument(file.name)) throw LocalRefused("$id is not an LCL document name")
        if (base != null && file.isFile) {
            val current = file.readBytes()
            val now = sha256Hex(current)
            if (now != base) throw LocalConflict(String(current, Charsets.UTF_8), now)
        }
        file.parentFile?.mkdirs()
        replace(file, text)
        return LocalDocument(id, text, digest(text))
    }

    /** Delete a document, only while it still holds the bytes with [digest]. */
    fun delete(project: String, id: String, digest: String) {
        val file = resolve(project, id)
        if (!file.isFile || !LclNames.isDocument(file.name)) throw LocalRefused("$id is not a document of the project")
        if (sha256Hex(file.readBytes()) != digest) throw LocalRefused("$id changed since it was shown, so it was left alone")
        if (!file.delete()) throw LocalRefused("$id could not be deleted")
        removeSyncRecord(project, id)
    }

    /** Delete an empty folder. */
    fun deleteFolder(project: String, id: String) {
        val dir = resolve(project, id)
        if (id.isEmpty() || !dir.isDirectory) throw LocalRefused("$id is not a folder of the project")
        if (dir.listFiles().orEmpty().any { !it.name.startsWith(".") }) throw LocalRefused("$id is not empty")
        if (!dir.deleteRecursively()) throw LocalRefused("$id could not be deleted")
    }

    /** The bytes reach the disk beside the target and replace it in one rename. */
    private fun replace(target: File, text: String) {
        val temporary = File(target.parentFile, ".${target.name}.${System.nanoTime()}.tmp")
        try {
            FileOutputStream(temporary).use { out ->
                out.write(text.toByteArray(Charsets.UTF_8))
                out.flush()
                out.fd.sync()
            }
            Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } catch (e: Exception) {
            temporary.delete()
            throw LocalRefused("${target.name} could not be written: ${e.message}")
        }
    }

    // --------------------------------------------------------------------- sync

    private fun syncFile(project: String): File = File(projectDir(project), SYNC_FILE)

    fun syncRecords(project: String): Map<String, SyncRecord> {
        val file = syncFile(project)
        if (!file.isFile) return emptyMap()
        val json = runCatching { Json.parseToJsonElement(file.readText()) as? JsonObject }.getOrNull() ?: return emptyMap()
        return json.mapNotNull { (id, value) ->
            val o = value as? JsonObject ?: return@mapNotNull null
            SyncRecord(
                localDigest = o.str("local_digest") ?: return@mapNotNull null,
                pcProject = o.str("pc_project") ?: "",
                pcPath = o.str("pc_path") ?: "",
                pcDigest = o.str("pc_digest") ?: "",
                at = o.long("at") ?: 0L,
            ).let { id to it }
        }.toMap()
    }

    /** A PC confirmed it holds [id]'s bytes (digest [localDigest]) at [pcPath]; only then is this called. */
    fun recordSynced(project: String, id: String, localDigest: String, pcProject: String, pcPath: String, pcDigest: String, at: Long) {
        val records = syncRecords(project) + (id to SyncRecord(localDigest, pcProject, pcPath, pcDigest, at))
        writeSyncRecords(project, records)
    }

    private fun removeSyncRecord(project: String, id: String) {
        val records = syncRecords(project)
        if (id in records) writeSyncRecords(project, records - id)
    }

    private fun writeSyncRecords(project: String, records: Map<String, SyncRecord>) {
        val json = buildJsonObject {
            for ((id, r) in records) {
                put(id, buildJsonObject { put("local_digest", r.localDigest); put("pc_project", r.pcProject); put("pc_path", r.pcPath); put("pc_digest", r.pcDigest); put("at", r.at) })
            }
        }
        replace(syncFile(project), json.toString())
    }

    /** Whether [id]'s bytes now are the ones a PC confirmed. */
    fun isSynced(project: String, id: String): Boolean {
        val record = syncRecords(project)[id] ?: return false
        return runCatching { read(project, id).digest == record.localDigest }.getOrDefault(false)
    }

    /** Whether every document of the project is synced as it is now (an empty project is not). */
    fun fullySynced(project: String): Boolean {
        val documents = documents(project)
        val records = syncRecords(project)
        return documents.isNotEmpty() && documents.all { id -> records[id]?.localDigest == read(project, id).digest }
    }
}
