package io.lcl.workspace.workspace

import io.lcl.workspace.connection.ConnectionManager
import io.lcl.workspace.connection.ConnectionState
import io.lcl.workspace.local.LocalProjects
import io.lcl.workspace.local.LocalRefused
import io.lcl.workspace.local.SyncRecord
import io.lcl.workspace.remote.Reply
import io.lcl.workspace.remote.str
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject

/**
 * Sync needs the PC's explorer operations (`children`, `mkdir`), which came with LCL 0.5.1; an
 * older PC answers "unknown operation".
 */
const val SYNC_NEEDS_PC = "Syncing local projects requires LCL 0.5.1 or newer on the PC."

/**
 * The refusal for [doing] while documents have unsaved edits, naming each of them; null when
 * [unsaved] is empty.
 */
internal fun unsavedRefusal(unsaved: List<String>, doing: String): String? {
    if (unsaved.isEmpty()) return null
    return "Save these documents before $doing:\n" + unsaved.joinToString("\n") { "• $it" }
}

/**
 * Syncs a project kept on this phone into a PC project, when the person asks.
 *
 * The rules it keeps:
 * - A sync sends what is on the phone's disk, so it is refused while a document of the project has
 *   unsaved edits.
 * - It works from the project's complete inventory, read from the disk, never from the explorer's
 *   listing, which is cut at 4096 entries per folder.
 * - An entry counts as synced only once the PC's answer confirms it.
 * - A record of an earlier sync proves what a PC held then, not now; [verifyRemovable] asks again.
 *
 * Nothing here removes the phone's copy. The controller does that, and only after [verifyRemovable]
 * or a complete sync of the whole project.
 */
internal class ProjectSync(
    private val connection: ConnectionManager,
    private val storage: LocalProjects,
    private val io: CoroutineDispatcher,
    /** The time now, in seconds, for the sync records. */
    private val now: () -> Long,
    /** The documents of a project open with unsaved edits, by id. */
    private val unsavedIn: (projectId: String) -> List<String>,
) {
    // ------------------------------------------------------------------ status

    /** Where the project stands against a PC, in a few words, for the Files pane. */
    suspend fun status(projectId: String): String {
        val unsaved = unsavedIn(projectId).size
        if (unsaved > 0) {
            val documents = if (unsaved == 1) "document" else "documents"
            return "Unsaved changes: $unsaved $documents — save before syncing or removing"
        }
        val name = LocalProjects.nameOf(projectId)
        return withContext(io) {
            try {
                val inventory = storage.inventory(name)
                val entries = inventory.folders + inventory.documents
                val synced = entries.count { storage.isSynced(name, it) }
                val pending = entries.size - synced
                when {
                    entries.isEmpty() -> "Empty"
                    pending == 0 -> "Synced — the PC confirms again before removal"
                    storage.syncRecords(name).isEmpty() -> "Local — not synced to a PC yet"
                    else ->
                        "Changed since sync — $pending of ${entries.size} " +
                            "not on a PC as they are now"
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                "Local — ${e.message}"
            }
        }
    }

    // ---------------------------------------------------------------- planning

    /**
     * Finds out what syncing [localProject] (or only [ids] of it) into [folder] of [pcProject]
     * would meet, without writing anything.
     *
     * For each document the PC holds either nothing at its destination, the same bytes, other
     * bytes, or the version this phone last synced there. In the last case the destination is the
     * one that sync used, so a copy saved beside the PC's file stays beside it, and the phone's
     * newer text replaces it without a question.
     *
     * Fails, with the reason, when the PC is not connected or is older than LCL 0.5.1, when a
     * document has unsaved edits, or when the project is too large to inventory.
     */
    suspend fun plan(
        localProject: ProjectInfo,
        pcProject: ProjectInfo,
        folder: String,
        ids: List<String>?,
    ): Result<SyncPlan> =
        try {
            Result.success(planOrRefuse(localProject, pcProject, folder, ids))
        } catch (e: LocalRefused) {
            Result.failure(e)
        }

    private suspend fun planOrRefuse(
        localProject: ProjectInfo,
        pcProject: ProjectInfo,
        folder: String,
        ids: List<String>?,
    ): SyncPlan {
        if (!localProject.local || pcProject.local) {
            throw LocalRefused("Sync goes from a project on this phone to one on the PC.")
        }
        if (connection.state.value !is ConnectionState.Connected) {
            throw LocalRefused("Connect to the PC first.")
        }
        unsavedRefusal(unsavedIn(localProject.id), "syncing")?.let { throw LocalRefused(it) }

        val where = folder.trim().trim('/')
        if (where.split('/').any { it == ".." || it.startsWith(".") }) {
            throw LocalRefused("\"$where\" is not a folder inside the PC project.")
        }
        requireExplorerOperations(pcProject)

        val name = LocalProjects.nameOf(localProject.id)
        val (inventory, records) =
            withContext(io) { storage.inventory(name) to storage.syncRecords(name) }
        val whole = ids == null
        val plan =
            SyncPlan(
                project = localProject,
                pcProject = pcProject,
                folder = where,
                items = emptyList(),
                // A sync of chosen documents makes only the folders on their way.
                folders = if (whole) inventory.folders else emptyList(),
                whole = whole,
            )
        val items =
            (ids ?: inventory.documents).map { id ->
                val document = withContext(io) { storage.read(name, id) }
                planDocument(plan, id, document.text, document.digest, records[id])
            }
        return plan.copy(items = items)
    }

    /** Refuses a PC that lacks the operations a sync needs to make folders. */
    private suspend fun requireExplorerOperations(pcProject: ProjectInfo) {
        val probe = pc("children", fields(pcProject.id, "parent" to ""))
        if (probe.status == 400 && probe.error?.contains("unknown operation") == true) {
            throw LocalRefused(SYNC_NEEDS_PC)
        }
        if (!probe.ok) {
            throw LocalRefused("The PC refused ${pcProject.name}: ${probe.error ?: probe.status}")
        }
    }

    /** What the PC holds where one document would go. */
    private suspend fun planDocument(
        plan: SyncPlan,
        id: String,
        text: String,
        digest: String,
        record: SyncRecord?,
    ): SyncItem {
        val pcProject = plan.pcProject.id
        val usual = plan.destination(id)

        // Where the last sync into this folder put it, while the PC still holds exactly what it
        // confirmed then.
        val last = record?.takeIf {
            it.pcProject == pcProject && FileTree.parentOf(it.pcPath) == FileTree.parentOf(usual)
        }
        if (last != null) {
            val there = pc("open", fields(pcProject, "document" to last.pcPath))
            if (there.ok && there.obj.str("digest") == last.pcDigest) {
                val state =
                    if (digest == last.localDigest) SyncState.IDENTICAL else SyncState.SUPERSEDED
                val pcText = there.obj.str("text")
                return SyncItem(id, last.pcPath, text, digest, state, last.pcDigest, pcText)
            }
        }

        val reply = pc("open", fields(pcProject, "document" to usual))
        return when {
            reply.ok -> {
                val pcDigest = reply.obj.str("digest") ?: ""
                val state = if (pcDigest == digest) SyncState.IDENTICAL else SyncState.DIFFERENT
                SyncItem(id, usual, text, digest, state, pcDigest, reply.obj.str("text"))
            }
            reply.status == 404 -> SyncItem(id, usual, text, digest, SyncState.ABSENT)
            else -> throw LocalRefused("The PC refused $usual: ${reply.error ?: reply.status}")
        }
    }

    // ----------------------------------------------------------------- running

    /**
     * Carries out a plan and says how every entry ended.
     *
     * The folders are made first, empty ones too, then each document is written the way the person
     * chose. A lost connection stops the rest; what failed is reported as it is, the phone's copy
     * stays whole, and running the sync again is safe, because what reached the PC is then found
     * identical. At the end the disk is read again: a document edited meanwhile, or an entry that
     * appeared or went, makes the sync incomplete.
     */
    suspend fun run(plan: SyncPlan, choices: Map<String, SyncChoice>): SyncResult {
        unsavedRefusal(unsavedIn(plan.project.id), "syncing")?.let {
            return SyncResult(emptyList(), complete = false, removed = false, problem = it)
        }
        val name = LocalProjects.nameOf(plan.project.id)
        val progress = Progress()
        makeFolders(plan, progress)
        for (item in plan.items) {
            progress.outcomes += sendDocument(plan, item, choices[item.id], progress)
        }
        val outcomes = progress.outcomes

        // One write for all confirmations, not one per entry. A confirmation lost to a crash
        // before this is found again by the next sync, which asks the PC.
        try {
            withContext(io) { storage.recordSyncedAll(name, progress.confirmed) }
        } catch (e: LocalRefused) {
            val problem = "The phone could not record what the PC confirmed: ${e.message}"
            return SyncResult(outcomes, complete = false, removed = false, problem = problem)
        }

        val changed = changedDuringSync(plan, name, outcomes)
        val complete =
            changed.isEmpty() && outcomes.all { it.ok } && (outcomes.isNotEmpty() || plan.whole)
        val problem =
            if (changed.isEmpty()) {
                null
            } else {
                "Changed on the phone during the sync, so not synced: ${changed.joinToString(", ")}"
            }
        return SyncResult(outcomes, complete, removed = false, problem = problem)
    }

    /** What a running sync has learned so far. */
    private class Progress {
        val outcomes = mutableListOf<SyncOutcome>()

        /** What the PC confirmed, by local id, to be recorded together at the end. */
        val confirmed = mutableMapOf<String, SyncRecord>()

        /** Folders the PC could not make; nothing under them is attempted. */
        val unreachable = mutableSetOf<String>()

        /** Why the connection was lost, once it was; nothing after that is attempted. */
        var lost: String? = null

        fun isUnder(unreachableFolder: String) = unreachable.any {
            unreachableFolder == it || unreachableFolder.startsWith("$it/")
        }
    }

    /** One writing request; a lost connection is noted in [progress] and answered with null. */
    private suspend fun send(op: String, request: JsonObject, progress: Progress): Reply? =
        try {
            connection.request(op, request)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            progress.lost = e.message ?: "connection lost"
            null
        }

    /**
     * Makes every folder the plan needs on the PC, shallowest first: the project's own folders and
     * those on the way to each destination. A folder that is there already is fine, once listing it
     * proves that it is a folder.
     */
    private suspend fun makeFolders(plan: SyncPlan, progress: Progress) {
        val pcProject = plan.pcProject.id
        // Destination on the PC -> the project's own folder it stands for.
        val own = plan.folders.associateBy { plan.destination(it) }
        val onTheWay = (plan.items.map { it.destination } + own.keys).flatMap(FileTree::ancestors)
        val needed =
            (own.keys + onTheWay)
                .distinct()
                .sortedWith(compareBy({ path -> path.count { it == '/' } }, { it }))

        for (folder in needed) {
            // Only the project's own folders are reported and recorded; the others merely lead
            // to a destination.
            val local = own[folder]
            fun failed(why: String) {
                if (local != null) progress.outcomes += SyncOutcome("$local/", folder, false, why)
            }

            val lost = progress.lost
            if (lost != null) {
                failed("not attempted: $lost")
                continue
            }
            if (progress.isUnder(folder)) {
                failed("its parent folder could not be made on the PC")
                continue
            }
            val reply = send("mkdir", fields(pcProject, "folder" to folder), progress)
            if (reply == null) {
                failed("not confirmed: ${progress.lost}")
                continue
            }
            val taken = reply.status == 409
            val isThere =
                reply.ok ||
                    (taken &&
                        pcOrNull("children", fields(pcProject, "parent" to folder))?.ok == true)
            if (!isThere) {
                progress.unreachable += folder
                failed(
                    if (taken) "something that is not a folder is at $folder on the PC"
                    else reply.error ?: "refused (${reply.status})"
                )
                continue
            }
            if (local != null) {
                // A folder's record has no digests: the PC has the folder.
                progress.confirmed[local] = SyncRecord("", pcProject, folder, "", now())
                val how = if (reply.ok) "made on the PC" else "already on the PC"
                progress.outcomes += SyncOutcome("$local/", folder, true, how)
            }
        }
    }

    /** A request that puts a document on the PC, and what to say once the PC confirmed it. */
    private class Write(
        val op: String,
        val request: JsonObject,
        val destination: String,
        val done: String,
    )

    /**
     * The request that puts [item] on the PC the way the person chose, or null when the PC's
     * version is kept.
     */
    private fun writeFor(item: SyncItem, choice: SyncChoice?, pcProject: String): Write? {
        fun create(destination: String) =
            Write(
                op = "create",
                request = fields(pcProject, "name" to destination, "text" to item.text),
                destination = destination,
                done = "created on the PC",
            )
        // A save names the digest it replaces, so the PC refuses it when the file changed since
        // the check.
        fun replace(done: String) =
            Write(
                op = "save",
                request =
                    fields(
                        pcProject,
                        "document" to item.destination,
                        "base" to (item.pcDigest ?: ""),
                        "text" to item.text,
                    ),
                destination = item.destination,
                done = done,
            )
        return when (item.state) {
            SyncState.ABSENT -> create(item.destination)
            SyncState.SUPERSEDED -> replace("updated on the PC")
            SyncState.DIFFERENT ->
                when (choice) {
                    SyncChoice.REPLACE -> replace("replaced on the PC")
                    SyncChoice.RENAME -> create(item.renamed)
                    SyncChoice.KEEP_PC,
                    null -> null
                }
            SyncState.IDENTICAL -> error("an identical document is not written")
        }
    }

    /**
     * Sends one document as planned. A confirmation goes into [progress]; the outcome is returned.
     */
    private suspend fun sendDocument(
        plan: SyncPlan,
        item: SyncItem,
        choice: SyncChoice?,
        progress: Progress,
    ): SyncOutcome {
        fun failed(why: String, destination: String = item.destination) =
            SyncOutcome(item.id, destination, false, why)

        progress.lost?.let {
            return failed("not attempted: $it")
        }
        if (progress.isUnder(FileTree.parentOf(item.destination))) {
            return failed("its folder could not be made on the PC")
        }

        val pcProject = plan.pcProject.id
        if (item.state == SyncState.IDENTICAL) {
            val pcDigest = item.pcDigest ?: item.localDigest
            progress.confirmed[item.id] =
                SyncRecord(item.localDigest, pcProject, item.destination, pcDigest, now())
            return SyncOutcome(item.id, item.destination, true, "already on the PC")
        }

        val write =
            writeFor(item, choice, pcProject)
                ?: return failed("kept the PC's version; the phone's copy is not on the PC")
        val reply =
            send(write.op, write.request, progress)
                ?: return failed("not confirmed: ${progress.lost}", write.destination)
        if (!reply.ok) {
            val why =
                if (reply.status == 409) {
                    "changed on the PC since it was checked, or exists; nothing was written"
                } else {
                    reply.error ?: "refused (${reply.status})"
                }
            return failed(why, write.destination)
        }

        // The confirmation is the PC's own digest of what it holds now: the bytes sent, or those
        // bytes with the final line feed the PC adds.
        val digest = reply.obj.str("digest")
        val accepted =
            if (item.text.endsWith("\n")) listOf(item.localDigest)
            else listOf(item.localDigest, LocalProjects.digest(item.text + "\n"))
        if (digest == null || digest !in accepted) {
            val why = "the PC reported other bytes than were sent; not marked synced"
            return failed(why, write.destination)
        }
        progress.confirmed[item.id] =
            SyncRecord(item.localDigest, pcProject, write.destination, digest, now())
        return SyncOutcome(item.id, write.destination, true, write.done)
    }

    /**
     * What changed on the phone while the sync ran, by reading the disk again: entries that
     * appeared or went (for a whole-project plan), and confirmed documents whose bytes are no
     * longer the ones sent.
     */
    private suspend fun changedDuringSync(
        plan: SyncPlan,
        name: String,
        outcomes: List<SyncOutcome>,
    ): List<String> {
        val changed = mutableListOf<String>()
        if (plan.whole) {
            try {
                val now = withContext(io) { storage.inventory(name) }
                val planned = plan.items.map { it.id }
                changed += (now.documents - planned.toSet()).map { "$it (new)" }
                changed += (planned - now.documents.toSet()).map { "$it (gone)" }
                changed += (now.folders - plan.folders.toSet()).map { "$it/ (new)" }
                changed += (plan.folders - now.folders.toSet()).map { "$it/ (gone)" }
            } catch (e: LocalRefused) {
                changed += e.message ?: "the project could not be read again"
            }
        }
        val confirmed = outcomes.filter { it.ok }.map { it.id }.toSet()
        for (item in plan.items) {
            if (item.id !in confirmed) continue
            val digestNow =
                withContext(io) { runCatching { storage.read(name, item.id).digest }.getOrNull() }
            if (digestNow != item.localDigest) changed += "${item.id} (edited)"
        }
        return changed
    }

    // ---------------------------------------------------------------- removing

    /**
     * Whether the project can leave the phone: nothing of it is unsaved here, and the PC, asked
     * now, holds every folder and every document as recorded — the same bytes at the same place.
     * Without the PC nothing is claimed.
     */
    suspend fun verifyRemovable(projectId: String): Result<Unit> =
        try {
            requireRemovable(projectId)
            Result.success(Unit)
        } catch (e: LocalRefused) {
            Result.failure(e)
        }

    private suspend fun requireRemovable(projectId: String) {
        val name = LocalProjects.nameOf(projectId)
        unsavedRefusal(unsavedIn(projectId), "removing $name from this phone")?.let {
            throw LocalRefused(it)
        }
        if (connection.state.value !is ConnectionState.Connected) {
            throw LocalRefused(
                "Connect to the PC first: without it the phone cannot confirm that the PC " +
                    "still holds $name."
            )
        }
        fun stays(why: String): Nothing = throw LocalRefused("$why, so $name stays on this phone.")

        val (inventory, records) =
            withContext(io) { storage.inventory(name) to storage.syncRecords(name) }
        for (folder in inventory.folders) {
            val record = records[folder] ?: stays("$folder/ was never synced to a PC")
            val reply =
                pcOrNull("children", fields(record.pcProject, "parent" to record.pcPath))
                    ?: stays("The PC did not answer")
            if (!reply.ok) {
                val detail = reply.error ?: reply.status
                stays("$folder/ is no longer on the PC at ${record.pcPath} ($detail)")
            }
        }
        for (document in inventory.documents) {
            val record = records[document]
            val digestHere = withContext(io) { storage.read(name, document).digest }
            if (record == null || record.localDigest != digestHere) {
                stays("$document is not on a PC as it is now")
            }
            val reply =
                pcOrNull("open", fields(record.pcProject, "document" to record.pcPath))
                    ?: stays("The PC did not answer")
            if (!reply.ok) {
                val detail = reply.error ?: reply.status
                stays("$document is no longer on the PC at ${record.pcPath} ($detail)")
            }
            if (reply.obj.str("digest") != record.pcDigest) {
                stays("$document changed on the PC at ${record.pcPath} since it was synced")
            }
        }
    }

    // ---------------------------------------------------------------- requests

    private suspend fun pcOrNull(op: String, fields: JsonObject): Reply? =
        connection.requestOrNull(op, fields)

    /** A request whose answer the plan needs; no answer is a refusal. */
    private suspend fun pc(op: String, fields: JsonObject): Reply =
        pcOrNull(op, fields) ?: throw LocalRefused("The PC did not answer.")
}
