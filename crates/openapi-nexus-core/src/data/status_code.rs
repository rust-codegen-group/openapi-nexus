use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum StatusCodeKind {
    Invalid,
    Exact(u16),
    Range { start: u16, end: u16 },
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct StatusCode {
    raw: String,
    kind: StatusCodeKind,
}

impl StatusCode {
    pub fn new<S: AsRef<str>>(status: S) -> Self {
        let raw = status.as_ref().trim().to_string();
        let upper = raw.to_uppercase();
        let kind = match upper.as_str() {
            "DEFAULT" => StatusCodeKind::Default,
            s if s.len() == 3 && s.ends_with("XX") => {
                let prefix = s.chars().next().and_then(|c| c.to_digit(10));
                if let Some(digit) = prefix {
                    let start = (digit as u16) * 100;
                    let end = start + 100;
                    StatusCodeKind::Range { start, end }
                } else {
                    StatusCodeKind::Invalid
                }
            }
            s => match s.parse::<u16>() {
                Ok(code) => StatusCodeKind::Exact(code),
                Err(_) => StatusCodeKind::Invalid,
            },
        };

        Self { raw, kind }
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn literal(&self) -> Option<u16> {
        match self.kind {
            StatusCodeKind::Exact(code) => Some(code),
            _ => None,
        }
    }

    pub fn range_bounds(&self) -> Option<(u16, u16)> {
        match self.kind {
            StatusCodeKind::Range { start, end } => Some((start, end)),
            _ => None,
        }
    }

    pub fn is_default(&self) -> bool {
        matches!(self.kind, StatusCodeKind::Default)
    }

    pub fn is_success(&self) -> bool {
        match self.kind {
            StatusCodeKind::Exact(code) => (200..300).contains(&code),
            StatusCodeKind::Range { start, .. } => (200..300).contains(&start),
            StatusCodeKind::Default | StatusCodeKind::Invalid => false,
        }
    }

    pub fn condition_expression(&self) -> Option<String> {
        match self.kind {
            StatusCodeKind::Exact(code) => Some(format!("response.status === {}", code)),
            StatusCodeKind::Range { start, end } => Some(format!(
                "response.status >= {} && response.status < {}",
                start, end
            )),
            StatusCodeKind::Default | StatusCodeKind::Invalid => None,
        }
    }

    fn sort_key(&self) -> (u8, u16, &str) {
        match self.kind {
            StatusCodeKind::Exact(code) => (0, code, self.raw.as_str()),
            StatusCodeKind::Range { start, .. } => (1, start, self.raw.as_str()),
            StatusCodeKind::Default => (2, 0, self.raw.as_str()),
            StatusCodeKind::Invalid => (3, 0, self.raw.as_str()),
        }
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl From<&str> for StatusCode {
    fn from(value: &str) -> Self {
        StatusCode::new(value)
    }
}

impl From<String> for StatusCode {
    fn from(value: String) -> Self {
        StatusCode::new(value)
    }
}

impl Ord for StatusCode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for StatusCode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
