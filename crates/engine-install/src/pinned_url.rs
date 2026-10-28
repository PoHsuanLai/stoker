//! The one kind of address a download is made from: `https`, with a host.

use url::Url;

/// The text is not an `https` address with a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not an https address with a host")]
pub struct NotPinned;

/// An `https` address with a host: the only kind the manifest pins and a redirect may lead to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedUrl(Url);

impl PinnedUrl {
    /// The address `text` names.
    ///
    /// # Errors
    /// [`NotPinned`] for text that is not a URL, is not `https`, or has no host.
    pub fn parse(text: &str) -> Result<Self, NotPinned> {
        let url = Url::parse(text).map_err(|_| NotPinned)?;
        (url.scheme() == "https" && url.host_str().is_some())
            .then_some(Self(url))
            .ok_or(NotPinned)
    }

    /// The address a `Location` header names from here: absolute, or relative to this one.
    ///
    /// # Errors
    /// [`NotPinned`] when it does not lead to an `https` address.
    pub fn join(&self, location: &str) -> Result<Self, NotPinned> {
        let joined = self.0.join(location).map_err(|_| NotPinned)?;
        Self::parse(joined.as_str())
    }

    /// The host, as a name or an address (an IPv6 one in brackets).
    pub fn host(&self) -> &str {
        self.0.host_str().unwrap_or_default()
    }

    /// The port: the one written, or 443.
    pub fn port(&self) -> u16 {
        self.0.port_or_known_default().unwrap_or(443)
    }

    /// The path and the query, as they go in the request line.
    pub fn path_and_query(&self) -> String {
        match self.0.query() {
            Some(query) => format!("{}?{query}", self.0.path()),
            None => self.0.path().to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_with_a_host_is_pinned() {
        for text in [
            "http://a.example/x",
            "ftp://a.example/x",
            "https://",
            "/x",
            "",
            "file:///x",
        ] {
            assert_eq!(PinnedUrl::parse(text), Err(NotPinned), "{text:?}");
        }
        let url = PinnedUrl::parse("https://a.example:8443/p/q%2Br.tar.gz?x=1").expect("pinned");
        assert_eq!(
            (url.host(), url.port(), url.path_and_query().as_str()),
            ("a.example", 8443, "/p/q%2Br.tar.gz?x=1")
        );
        assert_eq!(
            PinnedUrl::parse("https://a.example/x")
                .expect("pinned")
                .port(),
            443
        );
    }

    #[test]
    fn a_redirect_may_name_an_address_or_a_path_but_never_leave_https() {
        let from = PinnedUrl::parse("https://a.example/go").expect("pinned");
        let absolute = from.join("https://b.example/real").expect("absolute");
        assert_eq!(
            (absolute.host(), absolute.path_and_query().as_str()),
            ("b.example", "/real")
        );
        let path = from.join("/real?t=1").expect("path");
        assert_eq!(
            (path.host(), path.path_and_query().as_str()),
            ("a.example", "/real?t=1")
        );
        assert_eq!(from.join("http://b.example/real"), Err(NotPinned));
    }
}
