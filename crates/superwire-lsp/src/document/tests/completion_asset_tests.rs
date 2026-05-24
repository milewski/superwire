use super::*;

#[test]
fn suggests_asset_options_inside_asset_block() {
    let (source, cursor_position) = source_without_cursor_normalization(inline_document_template! {
        dynamic {
            video: asset "https://uxspot.cn/images/2020/05/uxspot-hero-video-preview.mp4" {
                <cursor>
            }
        }

        output {
            ok: true
        }
    });
    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert_completion_contains!(&completion_suggestions, "type", "media_type", "title", "context", "citations",);

    assert_completion_excludes_labels!(
        &completion_suggestions,
        AgentExpressionPropertyName::Instruction,
        InferenceSetting::Temperature
    );
}

#[test]
fn filters_asset_options_by_prefix() {
    let (source, cursor_position) = source_without_cursor_normalization(inline_document_template! {
        dynamic {
            image: asset input.image_url {
                med<cursor>
            }
        }

        output {
            ok: true
        }
    });
    let completion_suggestions = completion_suggestions_from_source(source, cursor_position);

    assert_completion_contains!(&completion_suggestions, "media_type");
    assert_completion_excludes_labels!(&completion_suggestions, "type", "title", "context", "citations");
}
