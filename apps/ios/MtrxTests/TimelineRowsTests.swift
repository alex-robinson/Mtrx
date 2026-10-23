import MtrxFFI
import XCTest
@testable import Mtrx

final class TimelineRowsTests: XCTestCase {
    func testGroupsMessagesBySenderAndTime() {
        let items = [
            Make.item("1", Make.event("alice", Make.text("Hi"), at: 0)),
            Make.item("2", Make.event("alice", Make.text("there"), at: 1)),
            Make.item("3", Make.event("alice", Make.text("later"), at: 10)),
            Make.item("4", Make.event("me", Make.text("Hello"), at: 11, own: true)),
        ]
        let rows = TimelineRows.build(items, showSenders: true)
        XCTAssertEqual(rows.map(\.id), ["1", "2", "3", "4"])
        let messages = rows.compactMap { row -> MessageRow? in
            if case .message(let message) = row.content { return message }
            return nil
        }
        XCTAssertEqual(messages.map(\.firstInGroup), [true, false, true, true])
        // Our own messages carry no name.
        XCTAssertEqual(messages.map(\.showSender), [true, false, true, false])
    }

    func testDirectChatsShowNoNames() {
        let rows = TimelineRows.build([Make.item("1", Make.event("alice", Make.text("Hi")))], showSenders: false)
        guard case .message(let message) = rows.first?.content else { return XCTFail("no message") }
        XCTAssertTrue(message.firstInGroup)
        XCTAssertFalse(message.showSender)
    }

    func testDayDividerStartsAGroup() {
        let items = [
            Make.item("1", Make.event("alice", Make.text("Hi"), at: 0)),
            TimelineItem(id: "d", content: .dayDivider(date: Make.start)),
            Make.item("2", Make.event("alice", Make.text("again"), at: 1)),
        ]
        let rows = TimelineRows.build(items, showSenders: true)
        XCTAssertEqual(rows[1].content, .day(Make.start))
        guard case .message(let message) = rows[2].content else { return XCTFail("no message") }
        XCTAssertTrue(message.firstInGroup)
    }

    func testFoldsStateEventsIntoOneLine() {
        let items = [
            Make.item("c", Make.event("alice", .otherState(eventType: "m.room.create"))),
            Make.item("j1", Make.event("alice", .membership(userId: "@alice:example.org", name: "Alice", change: .joined))),
            Make.item("n", Make.event("alice", .otherState(eventType: "m.room.name"))),
            Make.item("i", Make.event("alice", .membership(userId: "@bob:example.org", name: "Bob", change: .invited))),
            Make.item("j2", Make.event("bob", .membership(userId: "@bob:example.org", name: "Bob", change: .joined))),
            Make.item("m", Make.event("bob", Make.text("Hi all"))),
        ]
        let rows = TimelineRows.build(items, showSenders: true)
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(rows[0].id, "c")
        XCTAssertEqual(
            rows[0].content, .summary("Alice created and configured the room · Alice invited Bob · Bob joined"))
    }

    func testUndecryptableMessagesShowGroupedLikeMessages() {
        let items = [
            Make.item("1", Make.event("alice", Make.text("Hi"), at: 0)),
            Make.item("2", Make.event("alice", .unableToDecrypt, at: 1)),
        ]
        let rows = TimelineRows.build(items, showSenders: true)
        XCTAssertEqual(rows.map(\.id), ["1", "2"])
        guard case .message(let message) = rows[1].content else { return XCTFail("no message") }
        XCTAssertEqual(message.event.content, .unableToDecrypt)
        XCTAssertFalse(message.firstInGroup)
    }

    func testUndecryptableMessagesSayDecryptingWhileKeysArrive() {
        let undecryptable = Make.event("alice", .unableToDecrypt, eventId: "$1")
        XCTAssertEqual(messageRow(undecryptable)?.undecryptableText, "Could not decrypt this message.")
        XCTAssertEqual(messageRow(undecryptable, keysArriving: true)?.undecryptableText, "Decrypting…")
        // Other rows are the same either way.
        let text = Make.event("alice", Make.text("Hi"), eventId: "$2")
        XCTAssertNil(messageRow(text, keysArriving: true)?.undecryptableText)
        XCTAssertEqual(messageRow(text, keysArriving: true), messageRow(text))
    }

    private func messageRow(_ event: TimelineEvent, showSenders: Bool = true, keysArriving: Bool = false) -> MessageRow? {
        let rows = TimelineRows.build([Make.item("m", event)], showSenders: showSenders, keysArriving: keysArriving)
        if case .message(let message) = rows.first?.content { return message }
        return nil
    }

    func testRowCarriesReactionsQuoteEditedAndReaders() {
        let reactions = [
            Reaction(key: "👍", count: 2, byUs: true, senders: [Make.reader("me"), Make.reader("bob")]),
            Reaction(key: "🎉", count: 1, byUs: false, senders: [Make.reader("bob")]),
        ]
        let quote = ReplyQuote(
            eventId: "$orig", state: .loaded, senderId: "@bob:example.org", senderName: "Bob", kind: .text,
            body: "Hello")
        let event = Make.event(
            "alice", Make.text("Hi", edited: true, reactions: reactions, replyTo: quote), eventId: "$1",
            readBy: [Make.reader("bob"), Make.reader("carol")])
        guard let row = messageRow(event) else { return XCTFail("no message") }
        XCTAssertEqual(row.reactions, reactions)
        XCTAssertEqual(row.quote, quote)
        XCTAssertTrue(row.edited)
        XCTAssertEqual(row.readers.map(\.userId), ["@bob:example.org", "@carol:example.org"])
        // Someone else's, in a room: name, avatar and the avatars' column.
        XCTAssertTrue(row.showSender)
        XCTAssertTrue(row.avatarColumn)
    }

    func testAvatarColumnOnlyForOthersInRooms() {
        XCTAssertEqual(messageRow(Make.event("me", Make.text("Hi"), own: true))?.avatarColumn, false)
        XCTAssertEqual(messageRow(Make.event("bob", Make.text("Hi")), showSenders: false)?.avatarColumn, false)
    }

    func testMenuOffersWhatApplies() {
        let others = Make.event("bob", Make.text("Hi"), eventId: "$1")
        XCTAssertEqual(messageRow(others)?.actions, [.react, .reply, .copy, .selectText])

        let ours = Make.event("me", Make.text("Hi"), own: true, eventId: "$2", editable: true)
        XCTAssertEqual(messageRow(ours)?.actions, [.react, .reply, .copy, .selectText, .edit, .delete])

        // Not sent yet: no event ID, so no reaction, reply or deletion.
        let sending = Make.event("me", Make.text("Hi"), own: true, state: .sending, editable: true)
        XCTAssertEqual(messageRow(sending)?.actions, [.copy, .selectText, .edit])

        let failed = Make.event("me", Make.text("Hi"), own: true, state: .failed(error: "Too large"))
        XCTAssertEqual(messageRow(failed)?.actions, [.copy, .selectText, .retry, .discard])

        let deleted = Make.event("me", .redacted, own: true, eventId: "$3")
        XCTAssertEqual(messageRow(deleted)?.actions, [])

        let undecryptable = Make.event("me", .unableToDecrypt, own: true, eventId: "$4")
        XCTAssertEqual(messageRow(undecryptable)?.actions, [.delete])
    }

    func testOnlyMessagesThatCanBeRepliedToSwipeToReply() {
        XCTAssertEqual(messageRow(Make.event("bob", Make.text("Hi"), eventId: "$1"))?.canSwipeToReply, true)
        XCTAssertEqual(messageRow(Make.event("me", Make.text("Hi"), own: true, eventId: "$2"))?.canSwipeToReply, true)
        // Not sent yet (no event ID), refused, deleted, undecryptable.
        XCTAssertEqual(messageRow(Make.event("me", Make.text("Hi"), own: true, state: .sending))?.canSwipeToReply, false)
        let failed = Make.event("me", Make.text("Hi"), own: true, state: .failed(error: "Too large"))
        XCTAssertEqual(messageRow(failed)?.canSwipeToReply, false)
        XCTAssertEqual(messageRow(Make.event("bob", .redacted, eventId: "$3"))?.canSwipeToReply, false)
        XCTAssertEqual(messageRow(Make.event("bob", .unableToDecrypt, eventId: "$4"))?.canSwipeToReply, false)
    }

    func testSwipeToReplyBeginsLeftwardAndRepliesPastTheThreshold() {
        XCTAssertTrue(SwipeToReply.begins(velocity: CGPoint(x: -300, y: 40)))
        // Scrolling, the back gesture, and diagonal drags are left alone.
        XCTAssertFalse(SwipeToReply.begins(velocity: CGPoint(x: -100, y: 300)))
        XCTAssertFalse(SwipeToReply.begins(velocity: CGPoint(x: 300, y: 0)))
        XCTAssertFalse(SwipeToReply.begins(velocity: CGPoint(x: -100, y: -60)))

        var swipe = SwipeToReply()
        swipe.update(CGPoint(x: -30, y: 5))
        XCTAssertEqual(swipe.offset, -30)
        XCTAssertFalse(swipe.triggers)
        XCTAssertEqual(swipe.progress, 30 / SwipeToReply.threshold)
        swipe.update(CGPoint(x: -SwipeToReply.threshold, y: 0))
        XCTAssertTrue(swipe.triggers)
        XCTAssertEqual(swipe.progress, 1)
        // The bubble stops at the limit, and does not go right.
        swipe.update(CGPoint(x: -500, y: 0))
        XCTAssertEqual(swipe.offset, -SwipeToReply.limit)
        swipe.update(CGPoint(x: 40, y: 0))
        XCTAssertEqual(swipe.offset, 0)
        XCTAssertFalse(swipe.triggers)
    }

    func testReadersBeyondThreeShowAsACount() {
        let readers = ["a", "b", "c", "d", "e"].map { Make.reader($0) }
        let (shown, more) = ReaderAvatars.split(readers)
        XCTAssertEqual(shown.map(\.userId), readers.prefix(3).map(\.userId))
        XCTAssertEqual(more, 2)
        XCTAssertEqual(ReaderAvatars.split(Array(readers.prefix(2))).more, 0)
        // A name not known yet: the user ID's local part.
        XCTAssertEqual(Make.reader("anna", named: false).displayName, "anna")
        XCTAssertEqual(Make.reader("anna").displayName, "Anna")
    }

    func testQuoteSaysWhatTheOriginalIs() {
        func quote(_ state: QuoteState, kind: MessageKind? = .text, body: String? = "Hello", name: String? = "Bob")
            -> ReplyQuote
        {
            ReplyQuote(
                eventId: "$orig", state: state, senderId: state == .loaded ? "@bob:example.org" : nil,
                senderName: state == .loaded ? name : nil, kind: kind, body: body)
        }
        XCTAssertEqual(quote(.loading, body: nil).preview, "Loading…")
        XCTAssertFalse(quote(.loading, body: nil).canGoTo)
        XCTAssertEqual(quote(.failed, body: nil).preview, "Could not load the message")
        XCTAssertFalse(quote(.failed, body: nil).canGoTo)
        XCTAssertNil(quote(.failed, body: nil).senderLabel)

        let text = quote(.loaded)
        XCTAssertEqual(text.senderLabel, "Bob")
        XCTAssertEqual(text.preview, "Hello")
        XCTAssertTrue(text.canGoTo)
        XCTAssertEqual(quote(.loaded, name: nil).senderLabel, "bob")
        XCTAssertEqual(quote(.loaded, kind: .image, body: "cat.png").preview, "Image: cat.png")
        XCTAssertEqual(quote(.loaded, kind: .file, body: "").preview, "File")
        XCTAssertEqual(quote(.loaded, kind: nil, body: "Message deleted").preview, "Message deleted")
    }

    func testEmojiSearchByName() {
        XCTAssertTrue(Emoji.all.count > 1000)
        XCTAssertEqual(Set(Emoji.all.map(\.symbol)).count, Emoji.all.count)
        XCTAssertEqual(Emoji.all.first?.symbol, "😀")
        XCTAssertTrue(Emoji.search("tears joy").map(\.symbol).contains("😂"))
        XCTAssertFalse(Emoji.search("tears joy").map(\.symbol).contains("😀"))
        XCTAssertEqual(Emoji.search("  ").count, Emoji.all.count)
    }

    func testLocalpart() {
        XCTAssertEqual(localpart("@alice:example.org"), "alice")
        XCTAssertEqual(localpart("bob"), "bob")
    }
}
