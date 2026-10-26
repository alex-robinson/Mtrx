//! Links kept in a room's topic, one per line after its text, so every
//! client shows them:
//!
//! ```text
//! What the room is about.
//! [call] https://meet.google.com/abc-defg-hij
//! [call Weekly passcode 123] https://zoom.us/j/123456789
//! [link] https://drive.google.com/drive/folders/xyz
//! [link Paper draft] https://www.overleaf.com/project/abc
//! ```
//!
//! The first call is the room's: the header joins it.

use url::Url;

use crate::{CallService, recognize_call_link};

/// A room's topic: its text, and the links on lines of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Topic {
    /// The topic without its link lines.
    pub text: String,
    pub links: Vec<RoomLink>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomLink {
    pub kind: LinkKind,
    pub title: Option<String>,
    pub url: Url,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Call { passcode: Option<String> },
    Link,
}

impl Topic {
    /// Lines that are not links stay in the text, so nothing is lost.
    pub fn parse(topic: &str) -> Self {
        let mut text = Vec::new();
        let mut links = Vec::new();
        for line in topic.lines() {
            match RoomLink::parse(line) {
                Some(link) => links.push(link),
                None => text.push(line),
            }
        }
        Self {
            text: text.join("\n").trim().to_owned(),
            links,
        }
    }

    /// The text, then a line for each link, in order.
    pub fn to_topic(&self) -> String {
        let text = Some(self.text.trim().to_owned()).filter(|t| !t.is_empty());
        (text.into_iter())
            .chain(self.links.iter().map(RoomLink::line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn calls(&self) -> impl Iterator<Item = &RoomLink> {
        self.links.iter().filter(|l| l.is_call())
    }

    /// The room's call, which the header joins.
    pub fn default_call(&self) -> Option<&RoomLink> {
        self.calls().next()
    }
}

impl RoomLink {
    /// `[call] URL`, `[call Title passcode 123] URL`, `[link Title] URL`;
    /// the URL may be in angle brackets.
    fn parse(line: &str) -> Option<Self> {
        let (inside, url) = line.trim().strip_prefix('[')?.split_once(']')?;
        let url = url.trim();
        let url = (url.strip_prefix('<'))
            .and_then(|u| u.strip_suffix('>'))
            .unwrap_or(url);
        if url.contains(char::is_whitespace) {
            return None;
        }
        let url = Url::parse(url)
            .ok()
            .filter(|u| matches!(u.scheme(), "https" | "http"))?;
        let mut words = inside.split_whitespace();
        let keyword = words.next()?;
        let words: Vec<&str> = words.collect();
        let (title, kind) = if keyword.eq_ignore_ascii_case("call") {
            // The last "passcode" starts the passcode.
            match words
                .iter()
                .rposition(|w| w.eq_ignore_ascii_case("passcode"))
            {
                Some(at) if at + 1 < words.len() => (
                    &words[..at],
                    LinkKind::Call {
                        passcode: Some(words[at + 1..].join(" ")),
                    },
                ),
                _ => (&words[..], LinkKind::Call { passcode: None }),
            }
        } else if keyword.eq_ignore_ascii_case("link") {
            (&words[..], LinkKind::Link)
        } else {
            return None;
        };
        Some(Self {
            kind,
            title: Some(title.join(" ")).filter(|t| !t.is_empty()),
            url,
        })
    }

    /// The link's line in the topic.
    pub fn line(&self) -> String {
        let mut words = vec![match self.kind {
            LinkKind::Call { .. } => "call".to_owned(),
            LinkKind::Link => "link".to_owned(),
        }];
        words.extend(self.title.as_deref().map(one_line));
        if let LinkKind::Call {
            passcode: Some(passcode),
        } = &self.kind
        {
            words.push(format!("passcode {}", one_line(passcode)));
        }
        words.retain(|w| !w.is_empty());
        format!("[{}] {}", words.join(" "), self.url)
    }

    pub fn is_call(&self) -> bool {
        matches!(self.kind, LinkKind::Call { .. })
    }

    pub fn passcode(&self) -> Option<&str> {
        match &self.kind {
            LinkKind::Call { passcode } => passcode.as_deref(),
            LinkKind::Link => None,
        }
    }

    /// What the link is called: its title, else a call's service (among
    /// `services` and the well-known ones), else its host.
    pub fn name(&self, services: &[CallService]) -> String {
        if let Some(title) = &self.title {
            return title.clone();
        }
        (self.is_call())
            .then(|| recognize_call_link(self.url.as_str(), services))
            .flatten()
            .unwrap_or_else(|| self.host())
    }

    /// The host without "www.", e.g. "overleaf.com".
    pub fn host(&self) -> String {
        match self.url.host_str() {
            Some(host) => host.trim_start_matches("www.").to_owned(),
            None => self.url.to_string(),
        }
    }
}

/// `text` fit for inside the brackets: one line, no brackets.
fn one_line(text: &str) -> String {
    text.replace(['[', ']'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(kind: LinkKind, title: Option<&str>, url: &str) -> RoomLink {
        RoomLink {
            kind,
            title: title.map(Into::into),
            url: Url::parse(url).unwrap(),
        }
    }

    const CALL: LinkKind = LinkKind::Call { passcode: None };

    #[test]
    fn reads_links_from_their_lines() {
        let topic = Topic::parse(
            "PalMA-ice project chat.\n\
             [call] https://meet.google.com/abc-defg-hij\n\
             Weekly on Tuesdays.\n\
             [Call Weekly passcode 123 456] <https://zoom.us/j/1>\n\
             [link] https://drive.google.com/drive/folders/xyz\n\
             [link Paper draft] https://www.overleaf.com/project/abc\n",
        );
        assert_eq!(topic.text, "PalMA-ice project chat.\nWeekly on Tuesdays.");
        assert_eq!(
            topic.links,
            [
                link(CALL, None, "https://meet.google.com/abc-defg-hij"),
                link(
                    LinkKind::Call {
                        passcode: Some("123 456".into())
                    },
                    Some("Weekly"),
                    "https://zoom.us/j/1"
                ),
                link(
                    LinkKind::Link,
                    None,
                    "https://drive.google.com/drive/folders/xyz"
                ),
                link(
                    LinkKind::Link,
                    Some("Paper draft"),
                    "https://www.overleaf.com/project/abc"
                ),
            ]
        );
        assert_eq!(
            topic.default_call().map(|c| c.url.as_str()),
            Some("https://meet.google.com/abc-defg-hij")
        );
    }

    #[test]
    fn other_lines_stay_text() {
        for line in [
            "[call]",
            "[call] not a url",
            "[call] https://zoom.us/j/1 and more",
            "[call] mailto:a@b.c",
            "[meeting] https://zoom.us/j/1",
            "[] https://zoom.us/j/1",
            "Join at https://zoom.us/j/1",
        ] {
            let topic = Topic::parse(line);
            assert_eq!(
                (topic.text.as_str(), topic.links.len()),
                (line, 0),
                "{line}"
            );
        }
    }

    #[test]
    fn writes_the_lines_back() {
        let topic = Topic {
            text: "About.\n".into(),
            links: vec![
                link(
                    LinkKind::Call {
                        passcode: Some("123".into()),
                    },
                    Some("Weekly [Zoom]"),
                    "https://zoom.us/j/1",
                ),
                link(CALL, None, "https://meet.google.com/abc-defg-hij"),
                link(
                    LinkKind::Link,
                    Some("Draft\nv2"),
                    "https://drive.google.com",
                ),
            ],
        };
        let written = topic.to_topic();
        assert_eq!(
            written,
            "About.\n\
             [call Weekly Zoom passcode 123] https://zoom.us/j/1\n\
             [call] https://meet.google.com/abc-defg-hij\n\
             [link Draft v2] https://drive.google.com/"
        );
        let read = Topic::parse(&written);
        assert_eq!(read.text, "About.");
        assert_eq!(read.links[0].title.as_deref(), Some("Weekly Zoom"));
        assert_eq!(read.links[1], topic.links[1]);
        assert_eq!(
            Topic::parse(&written).to_topic(),
            written,
            "the same once tidied"
        );
        assert_eq!(
            Topic {
                text: String::new(),
                links: vec![link(LinkKind::Link, None, "https://a.org/")],
            }
            .to_topic(),
            "[link] https://a.org/"
        );
    }

    #[test]
    fn names_links() {
        let services = [CallService::fixed("My Zoom", "https://zoom.us/j/42").unwrap()];
        let name = |kind, title, url| link(kind, title, url).name(&services);
        assert_eq!(name(CALL, None, "https://zoom.us/j/42"), "My Zoom");
        assert_eq!(
            name(CALL, None, "https://meet.google.com/abc-defg-hij"),
            "Google Meet"
        );
        assert_eq!(name(CALL, Some("Weekly"), "https://zoom.us/j/42"), "Weekly");
        assert_eq!(name(CALL, None, "https://www.lab.org/call"), "lab.org");
        assert_eq!(
            name(LinkKind::Link, None, "https://meet.google.com/abc-defg-hij"),
            "meet.google.com",
            "a link is not a call"
        );
    }
}
