package org.mtrx.app.rooms

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.mtrx.app.model.RoomSort
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.model.roomIdsOf
import uniffi.mtrx.RoomKind
import uniffi.mtrx.RoomListState
import uniffi.mtrx.RoomSection
import uniffi.mtrx.RoomSummary
import uniffi.mtrx.SectionKind

// A space's list in the desktop's sections (`SpaceRooms.sections`), shown,
// folded and ordered as chosen on this phone; and the Notes to self at the
// top where the account says (`RoomListState.notesAtTop`).

/**
 * How this phone arranges one space's sections, by `RoomSection.key`:
 * their [order], those [hidden] and those [folded]. Keys not in [order]
 * are new to the phone. [groupFolded]: the space's group on the Rooms tab
 * is folded.
 */
data class SectionPrefs(
    val order: List<String> = emptyList(),
    val hidden: Set<String> = emptySet(),
    val folded: Set<String> = emptySet(),
    val groupFolded: Boolean = false,
)

/** The keys (`SpaceEntry.key`) of the Rooms tab's groups folded in [prefs]. */
fun foldedGroups(prefs: Map<String, SectionPrefs>): Set<String> = prefs.filterValues { it.groupFolded }.keys

/** A section as the space's list shows it: its rooms in order; [unread] for its heading while folded. */
data class ShownSection(
    val key: String,
    val title: String,
    val rooms: List<RoomSummary>,
    val folded: Boolean,
    val unread: Long,
    val mentions: Boolean,
)

/** A space's list: the Notes to self at the top, if pinned there, then the sections shown. */
data class SectionedList(val notes: RoomSummary?, val sections: List<ShownSection>) {
    val isEmpty: Boolean get() = notes == null && sections.isEmpty()
}

/** A section in the Show sheet: shown or not, in the phone's order. */
data class SectionChoice(val key: String, val title: String, val shown: Boolean)

/** A built-in section's name, as the desktop's; a category's own. */
fun sectionTitle(section: RoomSection): String = when (section.kind) {
    SectionKind.INVITES -> "Invites"
    SectionKind.FAVOURITES -> "Favourites"
    SectionKind.DIRECT_MESSAGES -> "Direct messages"
    SectionKind.ROOMS -> "Rooms"
    SectionKind.LOW_PRIORITY -> "Low priority"
    SectionKind.CATEGORY -> section.name.ifBlank { "Category" }
}

/**
 * The keys of [account] (the account's order) in the phone's [saved]
 * order; a key new to the phone takes its place in the account's order,
 * after the key before it there. Keys gone from the account are left out.
 */
fun orderedKeys(account: List<String>, saved: List<String>): List<String> {
    val result = saved.filter { it in account }.distinct().toMutableList()
    account.forEachIndexed { index, key ->
        if (key in result) return@forEachIndexed
        val before = account.subList(0, index).lastOrNull { it in result }
        result.add(before?.let { result.indexOf(it) + 1 } ?: 0, key)
    }
    return result
}

/** The Notes to self while pinned at the top (and listed), else null. */
fun notesAtTop(state: RoomListState): RoomSummary? {
    val id = state.notesRoomId ?: return null
    if (!state.notesAtTop) return null
    return state.rooms.firstOrNull { it.id == id }
}

/** The Chats tab: the Notes to self at the top if pinned there, and the direct chats below without it. */
fun chatList(state: RoomListState): Pair<RoomSummary?, List<RoomSummary>> {
    val notes = notesAtTop(state)
    return notes to chats(state).filter { it.id != notes?.id }
}

/**
 * The sections of [entry] as the account arranges them: a space's from
 * the library; for Home (and a space without them), Invites,
 * Direct messages and Rooms from its rooms.
 */
fun accountSections(state: RoomListState, entry: SpaceEntry): List<RoomSection> {
    val space = (entry as? SpaceEntry.Space)?.let { e -> state.spaces.firstOrNull { it.spaceId == e.id } }
    if (space != null && space.sections.isNotEmpty()) return space.sections
    val ids = state.roomIdsOf(entry) ?: return emptyList()
    val byId = state.rooms.associateBy { it.id }
    val rooms = ids.mapNotNull { byId[it] }
    fun section(key: String, kind: SectionKind, of: List<RoomSummary>) = RoomSection(key, kind, "", of.map { it.id })
    val (invites, joined) = rooms.partition { it.invited }
    val (direct, others) = joined.partition { it.kind == RoomKind.DIRECT }
    return listOf(
        section(INVITES, SectionKind.INVITES, invites),
        section(DIRECT_MESSAGES, SectionKind.DIRECT_MESSAGES, direct),
        section(ROOMS, SectionKind.ROOMS, others),
    )
}

/**
 * [entry]'s list: the Notes to self at the top of a space where pinned
 * there (and out of its sections), then the sections in the phone's order
 * ([prefs]), without those hidden or empty, each's rooms in the order
 * [sort].
 */
fun spaceList(state: RoomListState, entry: SpaceEntry, prefs: SectionPrefs?, sort: RoomSort): SectionedList {
    val sections = accountSections(state, entry)
    val notes = notesAtTop(state)?.takeIf { entry is SpaceEntry.Space && state.roomIdsOf(entry) != null }
    val byKey = sections.associateBy { it.key }
    val byId = state.rooms.associateBy { it.id }
    val saved = prefs ?: SectionPrefs()
    val shown = orderedKeys(sections.map { it.key }, saved.order).mapNotNull { key ->
        val section = byKey.getValue(key)
        val rooms = section.roomIds.filter { it != notes?.id }.mapNotNull { byId[it] }
        if (key in saved.hidden || rooms.isEmpty()) return@mapNotNull null
        ShownSection(
            key = key,
            title = sectionTitle(section),
            rooms = sorted(rooms, sort),
            folded = key in saved.folded,
            unread = rooms.sumOf { it.unreadMessages.toLong() },
            mentions = rooms.any { it.unreadMentions > 0uL },
        )
    }
    return SectionedList(notes, shown)
}

/** The Show sheet's sections: all of [entry]'s, empty ones too, in the phone's order. */
fun sectionChoices(state: RoomListState, entry: SpaceEntry, prefs: SectionPrefs?): List<SectionChoice> {
    val sections = accountSections(state, entry)
    val byKey = sections.associateBy { it.key }
    val saved = prefs ?: SectionPrefs()
    return orderedKeys(sections.map { it.key }, saved.order).map { key ->
        SectionChoice(key, sectionTitle(byKey.getValue(key)), shown = key !in saved.hidden)
    }
}

const val INVITES = "invites"
const val DIRECT_MESSAGES = "direct_messages"
const val ROOMS = "rooms"

/** Where the sections' arrangement is kept, per space (`SpaceEntry.key`), on this phone. */
interface SectionStore {
    /** The saved arrangements; null if none, or they cannot be read. */
    suspend fun load(): Map<String, SectionPrefs>?

    suspend fun save(prefs: Map<String, SectionPrefs>)
}

/**
 * How this phone arranges each space's sections (shown, folded, order)
 * and whether its group on the Rooms tab is folded, by `SpaceEntry.key`
 * (Home's too), saved in [store]. Not the account's: the desktop and
 * other devices keep theirs. Used on [scope]'s (main) thread. The changes
 * take the keys of the space's sections now ([keys], the account's order),
 * so that a reorder keeps sections new to the phone in place.
 */
class SectionLayouts(private val store: SectionStore, private val scope: CoroutineScope) {
    private val _prefs = MutableStateFlow<Map<String, SectionPrefs>>(emptyMap())
    val prefs: StateFlow<Map<String, SectionPrefs>> = _prefs.asStateFlow()

    /** Changed before the saved ones were read: those are stale. */
    private var changed = false

    // Saves run in order and each saves the latest.
    private val saving = Mutex()

    init {
        scope.launch {
            val saved = store.load()
            if (!changed && saved != null) _prefs.value = saved
        }
    }

    /** Fold [key], or unfold it. */
    fun toggleFold(space: String, key: String) = change(space) {
        it.copy(folded = if (key in it.folded) it.folded - key else it.folded + key)
    }

    /** Fold the space's group on the Rooms tab, or unfold it. */
    fun toggleGroupFold(space: String) = change(space) { it.copy(groupFolded = !it.groupFolded) }

    fun setShown(space: String, key: String, shown: Boolean) = change(space) {
        it.copy(hidden = if (shown) it.hidden - key else it.hidden + key)
    }

    fun showAll(space: String) = change(space) { it.copy(hidden = emptySet()) }

    fun hideAll(space: String, keys: List<String>) = change(space) { it.copy(hidden = keys.toSet()) }

    /** Move the section at [from] to [to], in the phone's order of [keys]. */
    fun move(space: String, keys: List<String>, from: Int, to: Int) = change(space) {
        val order = orderedKeys(keys, it.order).toMutableList()
        if (from !in order.indices || to !in order.indices || from == to) return@change it
        order.add(to, order.removeAt(from))
        it.copy(order = order)
    }

    private fun change(space: String, update: (SectionPrefs) -> SectionPrefs) {
        changed = true
        val current = _prefs.value[space] ?: SectionPrefs()
        _prefs.value = _prefs.value + (space to update(current))
        scope.launch { saving.withLock { store.save(_prefs.value) } }
    }
}
