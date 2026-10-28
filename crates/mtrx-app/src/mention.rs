//! Mentioning people while writing: `@` and the start of a name offers the
//! room's members to complete it with. A mention is text in the composer,
//! `@` + a display name (when no other member has it) or a user ID, or
//! `@room`; sending resolves it against the members (see
//! [`mtrx_core::mention_targets`]).

use std::{ops::Range, sync::Arc};

use iced::{
    Alignment, Element, Length,
    advanced::text::highlighter::Format,
    widget::{button, column, container, row, text, text_editor},
};
use mtrx_core::{Member, UserId};

use crate::{
    app,
    avatar::{Avatars, Shape},
    ellipsis::ellipsis,
    emoji, style,
};

/// Candidates offered at most.
const MAX: usize = 8;
/// Longest partial name completed.
const MAX_QUERY: usize = 40;
const AVATAR: f32 = 24.0;
const WIDTH: f32 = 340.0;

#[derive(Debug, Clone)]
pub enum Message {
    Up,
    Down,
    /// Complete with the highlighted candidate.
    Choose,
    /// Complete with this mention.
    Pick(String),
    Close,
}

/// A partial mention before the cursor: `@` at `start` (a byte) of `line`,
/// then `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub line: usize,
    pub start: usize,
    pub text: String,
}

/// The completion offered, while there is a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub query: Query,
    /// The highlighted candidate.
    pub selected: usize,
}

impl Completion {
    /// The completion for `content`'s cursor, keeping the highlighted
    /// candidate of `previous` while the same `@` is completed.
    pub fn of(content: &text_editor::Content, previous: Option<&Completion>) -> Option<Self> {
        let query = completion(content)?;
        let selected = previous
            .filter(|p| (p.query.line, p.query.start) == (query.line, query.start))
            .map_or(0, |p| p.selected);
        Some(Self { query, selected })
    }

    /// Move the highlight by `step` among `count` candidates, round.
    pub fn step(&mut self, step: isize, count: usize) {
        if count > 0 {
            self.selected = (self.selected as isize + step).rem_euclid(count as isize) as usize;
        }
    }
}

/// The partial mention just before the cursor, if any: an `@` not right
/// after a letter or digit, on the cursor's line, followed by at most
/// [`MAX_QUERY`] characters that do not start or end with a space (names
/// have spaces, but after a space nothing is being completed yet).
pub fn completion(content: &text_editor::Content) -> Option<Query> {
    let cursor = content.cursor();
    if cursor.selection.is_some() {
        return None;
    }
    let position = cursor.position;
    let line = content.line(position.line)?;
    let before = line.text.get(..position.column)?;
    let (start, _) = before
        .char_indices()
        .rev()
        .take(MAX_QUERY + 1)
        .find(|&(i, c)| c == '@' && !before[..i].chars().next_back().is_some_and(is_word))?;
    let text = &before[start + 1..];
    if text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace) {
        return None;
    }
    Some(Query {
        line: position.line,
        start,
        text: text.to_owned(),
    })
}

/// Who a query may mention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Candidate<'a> {
    Member {
        member: &'a Member,
        /// What completing puts in the text.
        mention: String,
    },
    Room,
}

impl Candidate<'_> {
    pub fn mention(&self) -> String {
        match self {
            Self::Member { mention, .. } => mention.clone(),
            Self::Room => "@room".into(),
        }
    }
}

/// The members whose display name or user name contains `query`, ignoring
/// case, those starting with it first; then `@room` if `room` (we may
/// notify everyone) and the query starts it. Not ourselves. `mentions` are
/// the texts a mention may follow `@` with: a name among them is unique and
/// completes as itself, others as the user ID.
pub fn candidates<'a>(
    query: &str,
    members: &'a [Member],
    mentions: &[String],
    own: &UserId,
    room: bool,
) -> Vec<Candidate<'a>> {
    let query = query.to_lowercase();
    let mut found: Vec<(bool, &Member)> = members
        .iter()
        .filter(|m| *m.id != *own)
        .filter_map(|m| {
            let name = m.name.as_deref().unwrap_or(m.id.localpart()).to_lowercase();
            let user = m.id.localpart().to_lowercase();
            let starts = name.starts_with(&query)
                || user.starts_with(&query)
                || name.split_whitespace().any(|word| word.starts_with(&query));
            (starts || name.contains(&query) || user.contains(&query)).then_some((starts, m))
        })
        .collect();
    // Stable: the member list's order within each.
    found.sort_by_key(|(starts, _)| !starts);
    let mut out: Vec<_> = found
        .into_iter()
        .take(MAX)
        .map(|(_, member)| {
            let mention = match member.name.as_deref() {
                Some(name) if mentions.iter().any(|m| m == name) => format!("@{name}"),
                _ => member.id.to_string(),
            };
            Candidate::Member { member, mention }
        })
        .collect();
    if room && "room".starts_with(&query) {
        out.truncate(MAX - 1);
        out.push(Candidate::Room);
    }
    out
}

/// Replace `query`'s partial mention before the cursor with `mention` and
/// a space: selected and pasted over, so that it undoes in one step.
pub fn insert(content: &mut text_editor::Content, query: &Query, mention: &str) {
    use text_editor::{Action, Cursor, Edit, Position};
    content.move_to(Cursor {
        position: content.cursor().position,
        selection: Some(Position {
            line: query.line,
            column: query.start,
        }),
    });
    content.perform(Action::Edit(Edit::Paste(Arc::new(format!("{mention} ")))));
}

/// The candidates, `selected` highlighted; a click completes with one.
pub fn view<'a>(
    candidates: &[Candidate<'_>],
    selected: usize,
    avatars: &Avatars,
) -> Element<'a, app::Message> {
    let rows = candidates.iter().enumerate().map(|(i, candidate)| {
        let (picture, name, detail): (Element<'a, app::Message>, String, String) = match candidate {
            Candidate::Member { member, .. } => {
                let name = member.name.as_deref().unwrap_or(member.id.localpart());
                (
                    avatars.view(
                        member.id.as_str(),
                        name,
                        member.avatar.as_deref(),
                        AVATAR,
                        Shape::Circle,
                    ),
                    name.to_owned(),
                    member.id.to_string(),
                )
            }
            Candidate::Room => (
                container(text("@").size(14)).center(AVATAR).into(),
                "@room".to_owned(),
                "Notify everyone in the room".to_owned(),
            ),
        };
        button(
            row![
                picture,
                ellipsis(name).size(13),
                ellipsis(detail)
                    .size(12)
                    .width(Length::Fill)
                    .style(text::secondary),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([3, 6])
        .style(style::list_item(i == selected))
        .on_press(app::Message::Mention(Message::Pick(candidate.mention())))
        .into()
    });
    container(column(rows).spacing(1))
        .width(WIDTH)
        .padding(4)
        .style(style::toolbar)
        .into()
}

/// What [`Highlighter`] marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    /// Drawn with the emoji font (see [`emoji::runs`]).
    Emoji,
    Mention,
}

/// Marks a text field's emoji, so they are drawn with the emoji font while
/// typing too, and its mentions: `@` + one of the settings' texts (see
/// [`mtrx_format::find_mentions`]).
pub struct Highlighter {
    mentions: Arc<[String]>,
    line: usize,
}

impl iced::advanced::text::Highlighter for Highlighter {
    type Settings = Arc<[String]>;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(mentions: &Self::Settings) -> Self {
        Self {
            mentions: mentions.clone(),
            line: 0,
        }
    }

    fn update(&mut self, mentions: &Self::Settings) {
        self.mentions = mentions.clone();
        self.line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.line += 1;
        highlights(line, &self.mentions).into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// The emoji and mentions of `line`, in order; emoji within a mention are
/// emoji.
fn highlights(line: &str, mentions: &[String]) -> Vec<(Range<usize>, Highlight)> {
    let found = mtrx_format::find_mentions(line, mentions);
    let mut out = Vec::new();
    let mut at = 0;
    for (is_emoji, run) in emoji::runs(line) {
        let range = at..at + run.len();
        at = range.end;
        if is_emoji {
            out.push((range, Highlight::Emoji));
            continue;
        }
        for (mention, _) in &found {
            let (start, end) = (mention.start.max(range.start), mention.end.min(range.end));
            if start < end {
                out.push((start..end, Highlight::Mention));
            }
        }
    }
    out
}

/// How [`Highlighter`]'s highlights are drawn.
pub fn format(highlight: &Highlight, theme: &iced::Theme) -> Format<iced::Font> {
    match highlight {
        Highlight::Emoji => Format {
            color: None,
            font: Some(crate::fonts::EMOJI),
        },
        Highlight::Mention => Format {
            color: Some(theme.extended_palette().primary.base.color),
            font: None,
        },
    }
}

/// The texts `@` may mention among `members`, for [`Highlighter`] and
/// [`candidates`].
pub fn texts(members: &[Member]) -> Arc<[String]> {
    mtrx_core::mention_targets(members.iter().map(|m| (&*m.id, m.name.as_deref())))
        .into_iter()
        .map(|(target, _)| target.text)
        .collect()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use mtrx_core::{OwnedUserId, Role};
    use text_editor::{Action, Content, Motion};

    use super::*;

    fn member(id: &str, name: Option<&str>) -> Member {
        Member {
            id: OwnedUserId::try_from(id).unwrap(),
            name: name.map(Into::into),
            avatar: None,
            role: Role::Member,
            invited: false,
            manage: Default::default(),
        }
    }

    /// `text` with the cursor at its end.
    fn typed(text: &str) -> Content {
        let mut content = Content::with_text(text);
        content.perform(Action::Move(Motion::DocumentEnd));
        content
    }

    #[test]
    fn completes_a_partial_name_before_the_cursor() {
        let query = |text| completion(&typed(text)).map(|q| (q.line, q.start, q.text));
        assert_eq!(query("hi @"), Some((0, 3, "".into())));
        assert_eq!(query("hi @Ni"), Some((0, 3, "Ni".into())));
        assert_eq!(query("line\n@Nils We"), Some((1, 0, "Nils We".into())));
        // Done with a space, not started, or an e-mail address.
        assert_eq!(query("@Nils "), None);
        assert_eq!(query("@ x"), None);
        assert_eq!(query("me@Ni"), None);
        assert_eq!(query("no mention"), None);
        assert_eq!(query(&format!("@{}", "x".repeat(MAX_QUERY + 1))), None);
    }

    #[test]
    fn offers_matching_members_then_the_room() {
        let members = [
            member("@me:x.org", Some("Me")),
            member("@weitzel:x.org", Some("Nils Weitzel")),
            member("@nils:x.org", Some("Nils")),
            member("@nils2:y.org", Some("Nils")),
            member("@ronja:x.org", None),
        ];
        let mentions = texts(&members);
        let own = members[0].id.clone();
        let offered = |query, room| {
            candidates(query, &members, &mentions, &own, room)
                .iter()
                .map(Candidate::mention)
                .collect::<Vec<_>>()
        };
        // Two people named Nils: they complete as their IDs.
        assert_eq!(
            offered("nils", false),
            ["@Nils Weitzel", "@nils:x.org", "@nils2:y.org"]
        );
        // A later word of the name, and the user name.
        assert_eq!(offered("weit", false), ["@Nils Weitzel"]);
        assert_eq!(offered("RON", true), ["@ronja:x.org"]);
        assert_eq!(offered("ro", true), ["@ronja:x.org", "@room"]);
        // Contained, not at the start, comes after.
        assert_eq!(offered("e", false), ["@Nils Weitzel"]);
        assert!(offered("me", false).is_empty(), "not ourselves");
    }

    #[test]
    fn completing_replaces_the_partial_name() {
        let mut content = typed("Thanks @Ni");
        let query = completion(&content).unwrap();
        insert(&mut content, &query, "@Nils Weitzel");
        assert_eq!(content.text(), "Thanks @Nils Weitzel ");
        assert_eq!(completion(&content), None);
    }

    #[test]
    fn highlights_mentions_and_emoji() {
        let mentions = ["Nils Weitzel".to_owned(), "room".to_owned()];
        assert_eq!(
            highlights("hi @Nils Weitzel 👍 @room", &mentions),
            [
                (3..16, Highlight::Mention),
                (17..21, Highlight::Emoji),
                (22..27, Highlight::Mention),
            ]
        );
    }
}
