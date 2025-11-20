//! Go documentation comments

use pretty::RcDoc;
use serde::{Deserialize, Serialize};

use crate::consts::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::ToRcDoc;

/// Go documentation comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoDocComment(pub String);

impl GoDocComment {
    pub fn new(comment: String) -> Self {
        Self(comment)
    }
}

impl ToRcDoc for GoDocComment {
    fn to_rcdoc(&self) -> RcDoc<'static, ()> {
        let lines: Vec<&str> = self.0.lines().collect();
        if lines.is_empty() {
            return RcDoc::nil();
        }

        if lines.len() == 1 && lines[0].len() + 3 <= MAX_LINE_WIDTH {
            // Single line comment
            RcDoc::text(format!("// {}", lines[0]))
        } else {
            // Multi-line comment
            let mut parts = vec![RcDoc::text("//")];
            for line in lines {
                parts.push(RcDoc::hardline());
                if line.is_empty() {
                    parts.push(RcDoc::text("//"));
                } else {
                    parts.push(RcDoc::text(format!("// {}", line)));
                }
            }
            RcDoc::concat(parts)
        }
    }
}
