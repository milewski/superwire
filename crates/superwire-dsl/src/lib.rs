pub use superwire_core::diagnostic;
pub use superwire_core::dsl::*;
pub use superwire_core::{parse_inline_workflow, workflow_source, workflow_source_template};

pub mod testing {
    pub use superwire_core::testing::{
        normalize_inline_cursor_layout, normalize_rust_doc_comment_tokens, InlineCursorPosition, SnapshotAssertion, WorkflowSourceTemplate,
        WorkflowSourceWithCursor, COMPACT_CURSOR_MARKER, SPACED_CURSOR_MARKER,
    };
}
