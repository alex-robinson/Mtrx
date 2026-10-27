//! Feedback: GitHub's form for a new issue, opened in the browser, which
//! signs in there. Posting from the app would need each person's GitHub
//! account, or a service of ours holding a token.

use mtrx_core::Url;

const NEW_ISSUE: &str = "https://github.com/mathesislab/Mtrx/issues/new";

/// The form, with the version and the system below the text to write.
pub fn url() -> String {
    let body = format!(
        "\n\n\nMtrx {}, {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    Url::parse_with_params(NEW_ISSUE, [("body", body)])
        .expect("a valid address")
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_with_the_version_below() {
        let url = Url::parse(&url()).unwrap();
        let (key, body) = url.query_pairs().next().unwrap();
        assert_eq!(key, "body");
        assert!(body.starts_with("\n\n\nMtrx "));
        assert!(body.contains(env!("CARGO_PKG_VERSION")));
    }
}
