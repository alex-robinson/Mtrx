package org.mtrx.app.screenshots

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import kotlinx.coroutines.flow.emptyFlow
import org.junit.Rule
import org.junit.Test
import org.mtrx.app.member
import org.mtrx.app.model.SpaceEntry
import org.mtrx.app.rooms.InviteContent
import org.mtrx.app.rooms.JoinContent
import org.mtrx.app.rooms.JoinResults
import org.mtrx.app.rooms.JoinState
import org.mtrx.app.rooms.MemberMenu
import org.mtrx.app.rooms.NewRoomContent
import org.mtrx.app.rooms.NewRoomForm
import org.mtrx.app.rooms.OverviewRooms
import org.mtrx.app.rooms.RoomInfoContent
import org.mtrx.app.rooms.SpaceOverviewContent
import org.mtrx.app.rooms.recentRows
import org.mtrx.app.rooms.recentUnread
import org.mtrx.app.rooms.SectionPrefs
import org.mtrx.app.rooms.chatList
import org.mtrx.app.rooms.sectionChoices
import org.mtrx.app.rooms.spaceList
import org.mtrx.app.ui.RoomListSettings
import org.mtrx.app.ui.SectionHeading
import org.mtrx.app.ui.ShowActions
import org.mtrx.app.ui.ShowSheetContent
import org.mtrx.app.rooms.spaceRows
import org.mtrx.app.rooms.spacesUnread
import org.mtrx.app.model.RoomSort
import org.mtrx.app.timeline.RoomHeader
import org.mtrx.app.timeline.TimelineContent
import org.mtrx.app.timeline.TimelineState
import org.mtrx.app.timeline.timelineRows
import org.mtrx.app.ui.ChatList
import org.mtrx.app.ui.HomeFrame
import org.mtrx.app.ui.HomeTab
import org.mtrx.app.ui.MtrxTheme
import org.mtrx.app.ui.RecentList
import org.mtrx.app.ui.SpaceList
import org.mtrx.app.ui.SpaceRoomsList
import org.mtrx.app.ui.SpaceTitle
import uniffi.mtrx.DirectoryKind
import uniffi.mtrx.Membership

/**
 * The key screens with fake data, as PNGs (recordPaparazziDebug; CI
 * uploads them as the android-screenshots artifact).
 */
class ScreensTest {
    @get:Rule
    val paparazzi = Paparazzi(deviceConfig = DeviceConfig.PIXEL_5)

    private val recentIds = listOf(ICE, PAPERS, TEACHING, LOOSE, WORKSHOP)
    private val badges = mapOf(HomeTab.Spaces to spacesUnread(rooms, recentIds), HomeTab.Rooms to recentUnread(rooms, recentIds))

    private fun snap(name: String, content: @Composable () -> Unit) {
        paparazzi.snapshot(name) { MtrxTheme { WithBack(content) } }
    }

    @Test
    fun chats() = snap("chats-notes-at-top") {
        val (notes, chats) = chatList(rooms)
        HomeFrame(HomeTab.Chats, badges, onTab = {}) {
            ChatList(notes, chats, loaded = true, images, today, onOpenRoom = {}, presenceOf = { null })
        }
    }

    @Test
    fun settingsSwitch() = snap("settings-notes-at-top") {
        HomeFrame(HomeTab.Settings, badges, onTab = {}) {
            SectionHeading("Room list")
            RoomListSettings(notesAtTop = true, onNotesAtTop = {})
        }
    }

    @Test
    fun showSheet() = snap("show-sheet") {
        Surface(Modifier.fillMaxWidth()) {
            ShowSheetContent(
                "Ice team",
                sectionChoices(rooms, SpaceEntry.Space(TEAM), SectionPrefs(hidden = setOf("low_priority"))),
                ShowActions(setShown = { _, _ -> }, showAll = {}, hideAll = {}, move = { _, _ -> }),
            )
        }
    }

    @Test
    fun spaces() = snap("spaces") {
        HomeFrame(HomeTab.Spaces, badges, onTab = {}) {
            SpaceList(spaceRows(rooms), SpaceEntry.Space(TEAM), loaded = true, images, onSelect = {})
        }
    }

    // All sections: Invites, Direct messages, Favourites, a category, Rooms
    // folded with its unread count, Low priority; the Notes to self on top.
    @Test
    fun spaceRooms() = snap("space-sections") {
        HomeFrame(HomeTab.Spaces, badges, onTab = {}, title = { SpaceTitle("Ice team", onClick = {}) }) {
            SpaceRoomsList(
                spaceList(rooms, SpaceEntry.Space(TEAM), SectionPrefs(folded = setOf("rooms")), RoomSort.DEFAULT),
                loaded = true,
                images,
                today,
                onOpenRoom = {},
                onFold = {},
            )
        }
    }

    @Test
    fun recent() = snap("recent") {
        HomeFrame(HomeTab.Rooms, badges, onTab = {}, title = { Text("Recent") }) {
            RecentList(recentRows(rooms, recentIds), images, today, onOpenRoom = {}, onBrowseSpaces = {})
        }
    }

    @Test
    fun recentEmpty() = snap("recent-empty") {
        HomeFrame(HomeTab.Rooms, emptyMap(), onTab = {}, title = { Text("Recent") }) {
            RecentList(emptyList(), images, today, onOpenRoom = {}, onBrowseSpaces = {})
        }
    }

    private val header = RoomHeader(
        name = "Ice sheets",
        avatarUrl = null,
        seed = ICE,
        person = null,
        topic = iceTopic,
        pinned = listOf("\$p1", "\$p2"),
    )

    @Composable
    private fun Room(topicOpen: Boolean = false, editingTopic: Boolean = false) {
        HomeFrame(HomeTab.Rooms, badges, onTab = {}, showTopBar = false) {
            TimelineContent(
                header,
                TimelineState(
                    rows = timelineRows(conversation),
                    draft = "Thanks Ana, looking at them now",
                    typing = listOf("Bob Berg"),
                    canEditTopic = true,
                    canPin = true,
                    pinned = timelineRows(pinnedMessages).filterIsInstance<org.mtrx.app.timeline.EventRow>(),
                ),
                NoTimelineActions,
                images,
                keysArriving = false,
                presence = null,
                backCount = 6,
                scrollTo = emptyFlow(),
                onBack = {},
                onRoomInfo = {},
                onOpenLink = {},
                topicOpen = topicOpen,
                editingTopic = editingTopic,
            )
        }
    }

    @Test
    fun room() = snap("room") { Room() }

    @Test
    fun topicPanel() = snap("topic-panel") { Room(topicOpen = true) }

    @Test
    fun topicEditor() = snap("topic-editor") { Room(editingTopic = true) }

    @Test
    fun memberMenu() = snap("member-menu") {
        Surface(Modifier.fillMaxWidth()) {
            MemberMenu(member("@bob:example.org", "Bob Berg", level = 50, upTo = 100, remove = true, ban = true), own = false, onChoose = {})
        }
    }

    @Test
    fun ownMemberMenu() = snap("member-menu-own") {
        Surface(Modifier.fillMaxWidth()) {
            MemberMenu(member(ME, "Me", level = 100, upTo = 99), own = true, onChoose = {})
        }
    }

    @Test
    fun spaceOverview() = snap("space-overview") {
        SpaceOverviewContent(
            teamHeader,
            OverviewRooms(teamChildren, updating = true, failed = false, pending = emptySet(), joined = setOf(ICE)),
            iceSettings,
            iceMembers,
            busy = null,
            error = null,
            ownUserId = ME,
            spaceNames = mapOf(TEAM to "Ice team", OUTREACH to "Outreach"),
            images = images,
            rowActions = noRowActions,
            infoActions = NoRoomInfoActions,
            onBack = {},
            onLeave = {},
            onOpenLink = {},
        )
    }

    // A list long enough to scroll, at its end: the last row clear of the pill.
    @Composable
    private fun OverviewAtEnd(updating: Boolean, failed: Boolean) {
        val long = teamChildren + (1..12).map { org.mtrx.app.directoryRoom("!extra$it:example.org", "Project $it", members = it.toLong()) } +
            org.mtrx.app.directoryRoom("!last:example.org", "Last room", members = 2, topic = "The list's last row")
        SpaceOverviewContent(
            teamHeader,
            OverviewRooms(long, updating = updating, failed = failed, pending = emptySet(), joined = setOf(ICE)),
            iceSettings,
            iceMembers,
            busy = null,
            error = null,
            ownUserId = ME,
            spaceNames = mapOf(TEAM to "Ice team", OUTREACH to "Outreach"),
            images = images,
            rowActions = noRowActions,
            infoActions = NoRoomInfoActions,
            onBack = {},
            onLeave = {},
            onOpenLink = {},
            // Past the end: the list stops at its last item.
            listState = rememberLazyListState(initialFirstVisibleItemIndex = long.size + 10),
        )
    }

    @Test
    fun spaceOverviewUpdatingAtEnd() = snap("space-overview-updating-end") { OverviewAtEnd(updating = true, failed = false) }

    @Test
    fun spaceOverviewNoAnswerAtEnd() = snap("space-overview-no-answer-end") { OverviewAtEnd(updating = false, failed = true) }

    @Test
    fun join() = snap("join") {
        JoinContent(
            JoinState(
                ownServer = "example.org",
                servers = listOf("matrix.org", "uni-hamburg.de"),
                server = null,
                query = "ice",
                kind = DirectoryKind.ALL,
                results = JoinResults.Listed(
                    listOf(
                        org.mtrx.app.directoryRoom("!glacio:example.org", "Glaciology", members = 240, topic = "Everything about glaciers"),
                        org.mtrx.app.directoryRoom("!ice:example.org", "Ice sheets", membership = Membership.JOINED, members = 12),
                        org.mtrx.app.directoryRoom("!polar:example.org", "Polar science", space = true, members = 80),
                        org.mtrx.app.directoryRoom("!icecore:example.org", "Ice cores", access = uniffi.mtrx.JoinAccess.KNOCK, members = 18),
                    ),
                    next = "2",
                ),
                pending = emptySet(),
                joined = setOf(ICE),
            ),
            images,
            noJoinActions,
            onBack = {},
        )
    }

    @Test
    fun newRoom() = snap("new-room") {
        NewRoomContent(
            space = false,
            parentName = "Ice team",
            form = NewRoomForm(
                name = "Field work 2027",
                topic = "Planning the Greenland traverse",
                invite = listOf("@ana:example.org", "@bob:example.org"),
                coOwners = setOf("@ana:example.org"),
            ),
            offersOwners = true,
            creating = false,
            error = null,
            actions = noNewRoomActions,
            onBack = {},
        )
    }

    @Test
    fun invite() = snap("invite") {
        InviteContent(rooms.rooms.first { it.id == WORKSHOP }.copy(topic = "Planning the 2027 ice sheet workshop"), images, busy = false, onAccept = {}, onDecline = {}, onBack = {})
    }
}

/** Room info is long: on a tall screen, to show its settings and members. */
class TallScreensTest {
    @get:Rule
    val paparazzi = Paparazzi(deviceConfig = DeviceConfig.PIXEL_5.copy(screenHeight = 2340 * 2))

    @Test
    fun roomInfo() {
        paparazzi.snapshot("room-info") {
            MtrxTheme {
                WithBack {
                    RoomInfoContent(
                        iceHeader,
                        iceSettings,
                        iceMembers,
                        busy = null,
                        error = null,
                        ownUserId = ME,
                        spaceNames = mapOf(TEAM to "Ice team", OUTREACH to "Outreach"),
                        images = images,
                        actions = NoRoomInfoActions,
                        onBack = {},
                        onLeave = {},
                        onOpenLink = {},
                    )
                }
            }
        }
    }
}
