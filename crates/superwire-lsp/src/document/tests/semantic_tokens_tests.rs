use std::sync::mpsc;
use std::time::Duration;

use crate::document::SemanticHighlightKind;

use lsp_types::{Position, Range};

use super::*;

#[test]
fn splits_crlf_multiline_string_tokens_at_exact_line_ranges_without_stalling() {
    let source = inline_document_template! {
        output {
            value: """
alpha
βeta
"""
        }
    }
    .replace('\n', "\r\n");
    let (highlight_sender, highlight_receiver) = mpsc::sync_channel(1);

    std::thread::spawn(move || {
        let document_state = DocumentState::new(source, None);
        let string_ranges = document_state
            .semantic_highlights()
            .into_iter()
            .filter(|highlight| highlight.kind == SemanticHighlightKind::String)
            .map(|highlight| highlight.range)
            .collect::<Vec<_>>();
        highlight_sender
            .send(string_ranges)
            .expect("semantic token test receiver should remain connected");
    });

    let string_ranges = highlight_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("CRLF multiline token splitting should complete without stalling");

    assert_eq!(
        string_ranges,
        vec![
            Range {
                start: Position { line: 0, character: 16 },
                end: Position { line: 0, character: 19 },
            },
            Range {
                start: Position { line: 1, character: 0 },
                end: Position { line: 1, character: 5 },
            },
            Range {
                start: Position { line: 2, character: 0 },
                end: Position { line: 2, character: 4 },
            },
            Range {
                start: Position { line: 3, character: 0 },
                end: Position { line: 3, character: 3 },
            },
        ]
    );
}
