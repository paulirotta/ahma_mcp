use super::AhmaMcpService;
use std::path::PathBuf;

impl AhmaMcpService {
    /// Parses a `file://` URI into a `PathBuf`.
    #[allow(dead_code)]
    pub(crate) fn parse_file_uri_to_path(uri: &str) -> Option<PathBuf> {
        const PREFIX: &str = "file://";
        if !uri.starts_with(PREFIX) {
            return None;
        }

        let mut rest = &uri[PREFIX.len()..];

        // Strip any query/fragment.
        if let Some(idx) = rest.find(['?', '#']) {
            rest = &rest[..idx];
        }

        // Accept host form: file://localhost/abs/path
        if let Some(after_localhost) = rest.strip_prefix("localhost") {
            rest = after_localhost;
        }

        // For unix-like paths we accept only absolute paths.
        if !rest.starts_with('/') {
            return None;
        }

        let decoded = Self::percent_decode_utf8(rest)?;
        Some(PathBuf::from(decoded))
    }

    /// Decodes a percent-encoded UTF-8 string.
    #[allow(dead_code)]
    pub(crate) fn percent_decode_utf8(input: &str) -> Option<String> {
        let bytes = input.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;

        while i < bytes.len() {
            match bytes[i] {
                b'%' => {
                    if i + 2 >= bytes.len() {
                        return None;
                    }
                    let hi = bytes[i + 1];
                    let lo = bytes[i + 2];

                    let hex = |b: u8| -> Option<u8> {
                        match b {
                            b'0'..=b'9' => Some(b - b'0'),
                            b'a'..=b'f' => Some(b - b'a' + 10),
                            b'A'..=b'F' => Some(b - b'A' + 10),
                            _ => None,
                        }
                    };

                    let hi = hex(hi)?;
                    let lo = hex(lo)?;
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }

        String::from_utf8(out).ok()
    }
}
