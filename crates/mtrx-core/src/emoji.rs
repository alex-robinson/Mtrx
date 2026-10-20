//! The emoji the user reacts with, most used first. Kept in account data
//! under Element's key, so both apps suggest the same ones.

use matrix_sdk::ruma::events::macros::EventContent;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{Result, Session};

/// Emoji remembered.
const KEEP: usize = 100;

#[derive(Clone, Debug, Default, Serialize, Deserialize, EventContent)]
#[ruma_event(type = "io.element.recent_emoji", kind = GlobalAccountData)]
struct RecentEmojiEventContent {
    /// Most recently used first, with how often.
    #[serde(default)]
    recent_emoji: Vec<(String, u64)>,
}

impl RecentEmojiEventContent {
    /// Most used first; equally used, most recent first.
    fn frequent(&self) -> Vec<String> {
        let mut emoji = self.recent_emoji.clone();
        emoji.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        emoji.into_iter().map(|(emoji, _)| emoji).collect()
    }

    fn record(&mut self, emoji: &str) {
        let count = match self.recent_emoji.iter().position(|(e, _)| e == emoji) {
            Some(at) => self.recent_emoji.remove(at).1,
            None => 0,
        };
        self.recent_emoji.insert(0, (emoji.to_owned(), count + 1));
        self.recent_emoji.truncate(KEEP);
    }
}

impl Session {
    /// The emoji the user reacts with, most used first.
    pub async fn frequent_emoji(&self) -> Vec<String> {
        match self
            .client
            .account()
            .account_data::<RecentEmojiEventContent>()
            .await
        {
            Ok(Some(raw)) => raw.deserialize().map(|c| c.frequent()).unwrap_or_default(),
            Ok(None) => Vec::new(),
            Err(e) => {
                warn!("could not read recent emoji: {e}");
                Vec::new()
            }
        }
    }

    /// Count a use of `emoji`. Reads the server's list first, so uses from
    /// other devices are kept.
    pub async fn record_emoji(&self, emoji: &str) -> Result<()> {
        let account = self.client.account();
        let mut content = account
            .fetch_account_data_static::<RecentEmojiEventContent>()
            .await?
            .and_then(|raw| raw.deserialize().ok())
            .unwrap_or_default();
        content.record(emoji);
        account.set_account_data(content).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_used_first_in_elements_format() {
        let mut content = RecentEmojiEventContent::default();
        for emoji in ["👍", "🎉", "👍", "😂"] {
            content.record(emoji);
        }
        assert_eq!(content.frequent(), ["👍", "😂", "🎉"]);
        assert_eq!(
            serde_json::to_value(&content).unwrap(),
            serde_json::json!({ "recent_emoji": [["😂", 1], ["👍", 2], ["🎉", 1]] })
        );
    }
}
