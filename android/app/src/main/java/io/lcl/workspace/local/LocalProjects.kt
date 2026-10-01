package io.lcl.workspace.local

import io.lcl.workspace.remote.long
import io.lcl.workspace.remote.sha256Hex
import io.lcl.workspace.remote.str
import io.lcl.workspace.workspace.LclNames
import io.lcl.workspace.workspace.LoadedFolder
import io.lcl.workspace.workspace.TreeEntry
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** A project kept on this phone, in the app's private storage, until it is synced to a PC. */
data class LocalProject(val name: String) {
    val id: String
        get() = LocalProjects.PREFIX + name
}

/** One document of a local project, as it is on the phone's storage now. */
data class LocalDocument(val id: String, val text: String, val digest: String)

/** What a PC confirmed for one local document: the phone bytes it holds a copy of, and where. */
data class SyncRecord(
    val localDigest: String,
    val pcProject: String,
    val pcPath: String,
    val pcDigest: String,
    val at: Long,
)

/**
 * Every folder and every document of a local project, by id: what a whole-project sync must carry.
 */
data class Inventory(val folders: List<String>, val documents: List<String>)

/** A local operation the storage refuses, and why. */
class LocalRefused(message: String) : Exception(message)

/** A write over a document that no longer holds the bytes it was loaded at. */
class LocalConflict(val text: String, val digest: String) :
    Exception("the document changed since it was loaded")

/**
 * Projects on this phone: folders under the app's private files directory (never cache, which
 * Android may clear), each holding LCL documents and folders exactly as a PC project would. The
 * phone has no LCL engine: this stores, lists and edits bytes, nothing more.
 *
 * Listing is lazy, one folder at a time, folders first then documents, every folder shown even
 * empty, dot entries never — the same explorer contract the PC serves. Every write is atomic: the
 * bytes go to a temporary file beside the target, are flushed to disk, and replace it in one
 * rename, so a save that fails leaves the previous version whole. Paths are resolved inside the
 * project only: `..`, absolute paths and dot names are refused.
 *
 * Syncing to a PC is recorded per document in the project's `.sync.json`: the digest of the local
 * bytes a PC confirmed it holds, and where; a folder the PC made or had is recorded the same way,
 * with no digest. A document counts as synced only while its bytes still have that digest. The
 * explorer's per-folder listing is cut at [MAX_CHILDREN] for the screen; a sync reads the whole
 * project through [inventory] instead, bounded on its own by [maxSyncItems], so nothing is ever
 * left out of a sync silently.
 */
class LocalProjects(private val root: File, private val maxSyncItems: Int = MAX_SYNC_ITEMS) {
    init {
        root.mkdirs()
    }

    companion object {
        const val PREFIX = "local:"
        const val MAX_CHILDREN = 4096
        /** Folders and documents together, at most, for one project's sync inventory. */
        const val MAX_SYNC_ITEMS = 20_000
        private const val SYNC_FILE = ".sync.json"

        fun isLocal(projectId: String): Boolean = projectId.startsWith(PREFIX)

        fun nameOf(projectId: String): String = projectId.removePrefix(PREFIX)

        /** The SHA-256 of a document's bytes, as the PC digests a file. */
        fun digest(text: String): String = sha256Hex(text.toByteArray(Charsets.UTF_8))
    }

    // ---------------------------------------------------------------- projects

    fun projects(): List<LocalProject> =
        root
            .listFiles()
            .orEmpty()
            .filter { it.isDirectory && !it.name.startsWith(".") }
            .map { LocalProject(it.name) }
            .sortedBy { it.name }

    fun exists(name: String): Boolean = runCatching {
        projectDir(name).isDirectory
    }
        .getOrDefault(false)

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
        if (
            name.isBlank() ||
                name.contains('/') ||
                name.contains('\\') ||
                name == "." ||
                name == ".." ||
                name.startsWith(".")
        ) {
            throw LocalRefused("\"$name\" is not a project name")
        }
        val dir = File(root, name)
        if (dir.canonicalFile.parentFile != root.canonicalFile)
            throw LocalRefused("\"$name\" is not a project name")
        return dir
    }

    /** The file for [id] inside [project], proven inside it; the project's folder for "". */
    private fun resolve(project: String, id: String): File {
        val dir = projectDir(project)
        if (id.isEmpty()) return dir
        val parts = id.split('/')
        for (part in parts) {
            if (
                part.isEmpty() ||
                    part == "." ||
                    part == ".." ||
                    part.startsWith(".") ||
                    part.contains('\\')
            ) {
                throw LocalRefused("\"$id\" is not a path inside the project")
            }
        }
        val file = File(dir, parts.joinToString(File.separator))
        val canonical = file.canonicalPath
        val base = dir.canonicalPath
        if (canonical != base && !canonical.startsWith(base + File.separator))
            throw LocalRefused("\"$id\" is outside the project")
        return file
    }

    private fun idOf(project: String, file: File): String =
        file.canonicalFile
            .relativeTo(projectDir(project).canonicalFile)
            .path
            .replace(File.separatorChar, '/')

    // ----------------------------------------------------------------- explorer

    /**
     * The direct children of one folder — every folder, then the LCL documents — and nothing below
     * them.
     */
    fun children(project: String, parent: String): LoadedFolder {
        val dir = resolve(project, parent)
        if (!dir.isDirectory) throw LocalRefused("there is no such folder in the project")
        val listed = dir.listFiles() ?: throw LocalRefused("the folder is not readable")
        val join = { name: String -> if (parent.isEmpty()) name else "$parent/$name" }
        val folders =
            listed.filter { it.isDirectory && !it.name.startsWith(".") }.map { it.name }.sorted()
        val documents =
            listed
                .filter { it.isFile && !it.name.startsWith(".") && LclNames.isDocument(it.name) }
                .map { it.name }
                .sorted()
        val entries =
            folders.map { TreeEntry(join(it), true) } + documents.map { TreeEntry(join(it), false) }
        return LoadedFolder(entries.take(MAX_CHILDREN), truncated = entries.size > MAX_CHILDREN)
    }

    /** Make one empty folder, under a folder that exists. */
    fun mkdir(project: String, id: String): String {
        val trimmed = id.trim().trimEnd('/')
        val dir = resolve(project, trimmed)
        if (dir.exists()) throw LocalRefused("$trimmed already exists")
        if (dir.parentFile?.isDirectory != true)
            throw LocalRefused("the parent folder of $trimmed does not exist")
        if (!dir.mkdir()) throw LocalRefused("$trimmed could not be created")
        return idOf(project, dir)
    }

    /**
     * Every folder and every document of the project, by id, deepest included: the complete
     * inventory a whole-project sync needs. Read from the disk directly, never through [children],
     * whose cut at [MAX_CHILDREN] is for the screen: a sync knows about every entry or stops. More
     * than [maxSyncItems] entries in all is refused, and the caller stops with everything kept.
     */
    fun inventory(project: String): Inventory {
        val folders = mutableListOf<String>()
        val documents = mutableListOf<String>()
        fun walk(parent: String) {
            val listed =
                resolve(project, parent).listFiles()
                    ?: throw LocalRefused(
                        "${parent.ifEmpty { "the project" }} is not readable, so the sync stops: nothing is written, marked synced or removed"
                    )
            val join = { name: String -> if (parent.isEmpty()) name else "$parent/$name" }
            for (entry in listed.sortedBy { it.name }) {
                if (entry.name.startsWith(".")) continue
                when {
                    entry.isDirectory -> {
                        folders += join(entry.name)
                        walk(join(entry.name))
                    }
                    entry.isFile && LclNames.isDocument(entry.name) -> documents += join(entry.name)
                }
                if (folders.size + documents.size > maxSyncItems) {
                    throw LocalRefused(
                        "the project has more than $maxSyncItems folders and documents, so the sync stops: nothing is written, marked synced or removed"
                    )
                }
            }
        }
        walk("")
        return Inventory(folders.sorted(), documents.sorted())
    }

    /** Every document of the project, by id: the documents of its [inventory]. */
    fun documents(project: String): List<String> = inventory(project).documents

    // ---------------------------------------------------------------- documents

    fun read(project: String, id: String): LocalDocument {
        val file = resolve(project, id)
        if (!file.isFile || !LclNames.isDocument(file.name))
            throw LocalRefused("$id is not a document of the project")
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
     * Write [text] over [id]. With [base], only when the file still holds the bytes with that
     * digest: otherwise nothing is written and the current text is reported ([LocalConflict]), as
     * the PC refuses a stale save.
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
        if (!file.isFile || !LclNames.isDocument(file.name))
            throw LocalRefused("$id is not a document of the project")
        if (sha256Hex(file.readBytes()) != digest)
            throw LocalRefused("$id changed since it was shown, so it was left alone")
        if (!file.delete()) throw LocalRefused("$id could not be deleted")
        removeSyncRecord(project, id)
    }

    /** Delete an empty folder. */
    fun deleteFolder(project: String, id: String) {
        val dir = resolve(project, id)
        if (id.isEmpty() || !dir.isDirectory)
            throw LocalRefused("$id is not a folder of the project")
        if (dir.listFiles().orEmpty().any { !it.name.startsWith(".") })
            throw LocalRefused("$id is not empty")
        if (!dir.deleteRecursively()) throw LocalRefused("$id could not be deleted")
        removeSyncRecord(project, id)
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
            Files.move(
                temporary.toPath(),
                target.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
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
        val json =
            runCatching { Json.parseToJsonElement(file.readText()) as? JsonObject }.getOrNull()
                ?: return emptyMap()
        return json
            .mapNotNull { (id, value) ->
                val o = value as? JsonObject ?: return@mapNotNull null
                SyncRecord(
                        localDigest = o.str("local_digest") ?: return@mapNotNull null,
                        pcProject = o.str("pc_project") ?: "",
                        pcPath = o.str("pc_path") ?: "",
                        pcDigest = o.str("pc_digest") ?: "",
                        at = o.long("at") ?: 0L,
                    )
                    .let { id to it }
            }
            .toMap()
    }

    /**
     * A PC confirmed it holds [id]'s bytes (digest [localDigest]) at [pcPath]; only then is this
     * called. For a folder, [localDigest] and [pcDigest] are empty: the PC has the folder.
     */
    fun recordSynced(
        project: String,
        id: String,
        localDigest: String,
        pcProject: String,
        pcPath: String,
        pcDigest: String,
        at: Long,
    ) {
        val records =
            syncRecords(project) + (id to SyncRecord(localDigest, pcProject, pcPath, pcDigest, at))
        writeSyncRecords(project, records)
    }

    /**
     * Many confirmations at once, in one write: a sync records each entry as it is confirmed and
     * writes them together at its end.
     */
    fun recordSyncedAll(project: String, confirmed: Map<String, SyncRecord>) {
        if (confirmed.isNotEmpty()) writeSyncRecords(project, syncRecords(project) + confirmed)
    }

    private fun removeSyncRecord(project: String, id: String) {
        val records = syncRecords(project)
        if (id in records) writeSyncRecords(project, records - id)
    }

    private fun writeSyncRecords(project: String, records: Map<String, SyncRecord>) {
        val json = buildJsonObject {
            for ((id, r) in records) {
                put(
                    id,
                    buildJsonObject {
                        put("local_digest", r.localDigest)
                        put("pc_project", r.pcProject)
                        put("pc_path", r.pcPath)
                        put("pc_digest", r.pcDigest)
                        put("at", r.at)
                    },
                )
            }
        }
        replace(syncFile(project), json.toString())
    }

    /** Whether [id] is now what a PC confirmed: a document's bytes, or a folder's presence. */
    fun isSynced(project: String, id: String): Boolean {
        val record = syncRecords(project)[id] ?: return false
        if (record.localDigest.isEmpty())
            return runCatching { resolve(project, id).isDirectory }.getOrDefault(false)
        return runCatching { read(project, id).digest == record.localDigest }.getOrDefault(false)
    }

    /**
     * Whether everything in the project is recorded as confirmed by a PC as it is now: every
     * document with its current bytes, every folder. A project with nothing in it has nothing to
     * lose; one too large to inventory is never fully synced. The records say what a PC held once;
     * whether it still does is the PC's to confirm again.
     */
    fun fullySynced(project: String): Boolean {
        val inventory = runCatching {
            inventory(project)
        }
            .getOrElse {
                return false
            }
        val records = syncRecords(project)
        return inventory.folders.all { records[it]?.localDigest?.isEmpty() == true } &&
            inventory.documents.all { id -> records[id]?.localDigest == read(project, id).digest }
    }
}
