use crate::document::DocumentSymbolNode;

use super::*;

#[test]
fn builds_recursive_variant_agent_dynamic_and_output_symbols() {
    let source = inline_document_template! {
        schema event {
            variant kind {
                created {
                    id: string
                }
                deleted {
                    reason: string
                }
            }
        }

        provider openai from openai {}

        model fast from openai {
            id: "gpt-4.1"
        }

        agent writer {
            model: model.fast
            instruction: "Write a report"
            output {
                report: string
            }
        }

        dynamic {
            metadata: {
                enabled: true
            }
        }

        output {
            result: {
                status: "ok"
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), None);
    let document_symbols = document_state.document_symbols();

    let event_symbol = symbol_named(&document_symbols, "event");
    let created_symbol = symbol_named(&event_symbol.children, "created");
    let writer_symbol = symbol_named(&document_symbols, "writer");
    let agent_output_symbol = symbol_named(&writer_symbol.children, "output");
    let dynamic_symbol = symbol_named(&document_symbols, "dynamic");
    let metadata_symbol = symbol_named(&dynamic_symbol.children, "metadata");
    let workflow_output_symbol = symbol_named(&document_symbols, "output");
    let result_symbol = symbol_named(&workflow_output_symbol.children, "result");

    assert!(event_symbol.children.iter().any(|symbol| symbol.name == "deleted"));
    assert!(created_symbol.children.iter().any(|symbol| symbol.name == "id"));
    assert!(agent_output_symbol.children.iter().any(|symbol| symbol.name == "report"));
    assert!(metadata_symbol.children.iter().any(|symbol| symbol.name == "enabled"));
    assert!(result_symbol.children.iter().any(|symbol| symbol.name == "status"));
}

fn symbol_named<'symbols>(symbols: &'symbols [DocumentSymbolNode], expected_name: &str) -> &'symbols DocumentSymbolNode {
    symbols
        .iter()
        .find(|symbol| symbol.name == expected_name)
        .unwrap_or_else(|| panic!("expected symbol `{expected_name}`; available symbols: {symbols:?}"))
}
