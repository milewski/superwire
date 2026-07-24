use super::{source_with_cursor, DocumentState};

#[test]
fn prefers_reference_field_semantics_over_colliding_builtin_type_name() {
    let source_template = inline_document_template! {
        input {
            string: number
        }

        output {
            value: input.<cursor>string
        }
    };
    let (source, cursor_position) = source_with_cursor(source_template);
    let document_state = DocumentState::new(source, None);
    let hover_markdown = document_state
        .hover_markdown(cursor_position)
        .expect("input field reference should provide hover information");

    assert!(
        hover_markdown.contains("Type: `number`"),
        "unexpected hover markdown: {hover_markdown}"
    );
    assert!(
        !hover_markdown.contains("String type"),
        "reference hover must not be shadowed by builtin documentation"
    );
}
