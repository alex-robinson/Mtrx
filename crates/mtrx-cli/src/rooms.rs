//! Room list rendering.

use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

use mtrx_core::{OwnedRoomId, RoomKind, RoomSummary};

/// Print spaces as a tree with their rooms, then rooms outside any space,
/// then direct messages and invites.
pub fn print_tree(rooms: &[RoomSummary], ids: bool) {
    let by_id: HashMap<&OwnedRoomId, &RoomSummary> = rooms.iter().map(|r| (&r.id, r)).collect();
    let mut children: HashMap<&OwnedRoomId, Vec<&RoomSummary>> = HashMap::new();
    for room in rooms {
        for parent in &room.parents {
            children.entry(parent).or_default().push(room);
        }
    }
    let in_list = |r: &RoomSummary| r.parents.iter().any(|p| by_id.contains_key(p));

    let top_spaces: Vec<_> = rooms
        .iter()
        .filter(|r| r.kind == RoomKind::Space && !r.invited && !in_list(r))
        .collect();
    for space in &top_spaces {
        print_node(space, &children, 0, &mut HashSet::new(), ids);
    }

    let loose = |r: &&RoomSummary, kind| !r.invited && r.kind == kind && !in_list(r);
    print_section(
        "Rooms",
        rooms.iter().filter(|r| loose(r, RoomKind::Room)),
        ids,
    );
    print_section(
        "Direct messages",
        rooms.iter().filter(|r| loose(r, RoomKind::Direct)),
        ids,
    );
    print_section("Invites", rooms.iter().filter(|r| r.invited), ids);
}

fn print_section<'a>(title: &str, rooms: impl Iterator<Item = &'a RoomSummary>, ids: bool) {
    let mut rooms = rooms.peekable();
    if rooms.peek().is_some() {
        println!("{title}");
        for room in rooms {
            println!("  {}", line(room, ids));
        }
    }
}

fn print_node<'a>(
    room: &'a RoomSummary,
    children: &HashMap<&OwnedRoomId, Vec<&'a RoomSummary>>,
    depth: usize,
    path: &mut HashSet<&'a OwnedRoomId>,
    ids: bool,
) {
    println!("{}{}", "  ".repeat(depth), line(room, ids));
    // Space graphs may contain cycles.
    if !path.insert(&room.id) {
        return;
    }
    for child in children.get(&room.id).into_iter().flatten() {
        print_node(child, children, depth + 1, path, ids);
    }
    path.remove(&room.id);
}

fn line(room: &RoomSummary, ids: bool) -> String {
    let marker = match room.kind {
        RoomKind::Space => "+",
        RoomKind::Room => "#",
        RoomKind::Direct => "@",
    };
    let mut out = format!("{marker} {}", room.name);
    if room.unread_mentions > 0 {
        let _ = write!(
            out,
            "  [{} unread, {} mentions]",
            room.unread_messages, room.unread_mentions
        );
    } else if room.unread_messages > 0 {
        let _ = write!(out, "  [{} unread]", room.unread_messages);
    }
    if room.encrypted {
        out += "  (e2ee)";
    }
    if ids {
        let _ = write!(out, "  {}", room.id);
    }
    out
}
