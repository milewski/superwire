#[macro_use]
mod common;

use common::{
    assert_publish_diagnostics_for_uri, character_index_for_fragment, did_close_params, did_open_params, line_index_with_fragments,
    text_document_formatting_params, text_document_params, text_document_position_params, LspProcessClient,
};
use serde_json::{json, Value};

const DOCUMENT_URI: &str = "file:///workspace/workflow.engine";

struct OpenFeatureDocument {
    language_server_client: LspProcessClient,
    document_text: String,
}

#[tokio::test]
async fn advertises_feature_capabilities() {
    let mut language_server_client = LspProcessClient::spawn();

    let initialize_response = initialize_server(&mut language_server_client, 201).await;

    assert_eq!(initialize_response["result"]["capabilities"]["definitionProvider"], true);
    assert_eq!(initialize_response["result"]["capabilities"]["documentSymbolProvider"], true);
    assert_eq!(initialize_response["result"]["capabilities"]["workspaceSymbolProvider"], true);
    assert_eq!(initialize_response["result"]["capabilities"]["foldingRangeProvider"], true);
    assert_eq!(initialize_response["result"]["capabilities"]["documentFormattingProvider"], true);

    shutdown_server(&mut language_server_client, 202).await;
}

#[tokio::test]
async fn supports_definition_requests() {
    let mut open_document = open_feature_document(301).await;
    let output_reference_line = line_index_with_fragments(&open_document.document_text, &["report", "agent", "writer"]);
    let agent_declaration_line = line_index_with_fragments(&open_document.document_text, &["agent", "writer", "{"]);
    let writer_character = character_index_for_fragment(&open_document.document_text, output_reference_line, "writer");

    let definition_response = open_document
        .language_server_client
        .send_request(
            302,
            "textDocument/definition",
            text_document_position_params(DOCUMENT_URI, output_reference_line, writer_character),
        )
        .await;

    assert_eq!(definition_response["jsonrpc"], "2.0");
    assert_eq!(definition_response["id"], 302);
    assert_eq!(definition_response["result"][0]["uri"], DOCUMENT_URI);
    assert_eq!(definition_response["result"][0]["range"]["start"]["line"], agent_declaration_line);

    close_feature_document(open_document, 303).await;
}

#[tokio::test]
async fn supports_document_and_workspace_symbols() {
    let mut open_document = open_feature_document(401).await;

    let document_symbol_response = open_document
        .language_server_client
        .send_request(402, "textDocument/documentSymbol", text_document_params(DOCUMENT_URI))
        .await;
    let document_symbol_names = symbol_names(&document_symbol_response);

    assert!(document_symbol_names.contains(&"openai"));
    assert!(document_symbol_names.contains(&"Report"));
    assert!(document_symbol_names.contains(&"writer"));

    let workspace_symbol_response = open_document
        .language_server_client
        .send_request(403, "workspace/symbol", json!({ "query": "wri" }))
        .await;
    let workspace_symbol_names = symbol_names(&workspace_symbol_response);

    assert!(workspace_symbol_names.contains(&"writer"));

    close_feature_document(open_document, 404).await;
}

#[tokio::test]
async fn supports_folding_and_formatting_requests() {
    let mut open_document = open_feature_document(501).await;

    let folding_range_response = open_document
        .language_server_client
        .send_request(502, "textDocument/foldingRange", text_document_params(DOCUMENT_URI))
        .await;

    assert!(folding_range_response["result"].is_array());

    let formatting_response = open_document
        .language_server_client
        .send_request(503, "textDocument/formatting", text_document_formatting_params(DOCUMENT_URI))
        .await;

    assert!(!formatting_response["result"]
        .as_array()
        .expect("formatting result should be an array")
        .is_empty());

    close_feature_document(open_document, 504).await;
}

#[tokio::test]
async fn supports_code_lens_and_execute_command_requests() {
    let mut open_document = open_feature_document(601).await;

    let code_lens_response = open_document
        .language_server_client
        .send_request(602, "textDocument/codeLens", text_document_params(DOCUMENT_URI))
        .await;
    let code_lens_titles = code_lens_response["result"]
        .as_array()
        .expect("code lens result should be an array")
        .iter()
        .filter_map(|code_lens| code_lens["command"]["title"].as_str())
        .collect::<Vec<_>>();

    assert!(code_lens_titles.contains(&"Generated output"));

    let execute_command_response = open_document
        .language_server_client
        .send_request(
            603,
            "workspace/executeCommand",
            json!({
                "command": "superwire.generated.output",
                "arguments": []
            }),
        )
        .await;

    assert_eq!(execute_command_response["result"], Value::Null);

    close_feature_document(open_document, 604).await;
}

async fn open_feature_document(request_id: u64) -> OpenFeatureDocument {
    let mut language_server_client = LspProcessClient::spawn();
    let document_text = feature_document_text();

    let _initialize_response = initialize_server(&mut language_server_client, request_id).await;

    language_server_client
        .send_notification("textDocument/didOpen", did_open_params(DOCUMENT_URI, &document_text))
        .await;

    let _diagnostics_notification = assert_publish_diagnostics_for_uri(&mut language_server_client, DOCUMENT_URI).await;

    OpenFeatureDocument {
        language_server_client,
        document_text,
    }
}

async fn initialize_server(language_server_client: &mut LspProcessClient, request_id: u64) -> Value {
    let initialize_response = language_server_client
        .send_request(request_id, "initialize", json!({ "capabilities": {} }))
        .await;

    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], request_id);

    initialize_response
}

async fn close_feature_document(mut open_document: OpenFeatureDocument, request_id: u64) {
    open_document
        .language_server_client
        .send_notification("textDocument/didClose", did_close_params(DOCUMENT_URI))
        .await;

    let _close_diagnostics_notification = assert_publish_diagnostics_for_uri(&mut open_document.language_server_client, DOCUMENT_URI).await;

    shutdown_server(&mut open_document.language_server_client, request_id).await;
}

async fn shutdown_server(language_server_client: &mut LspProcessClient, request_id: u64) {
    let shutdown_response = language_server_client.send_request(request_id, "shutdown", Value::Null).await;

    assert_eq!(shutdown_response["result"], Value::Null);

    language_server_client.send_notification("exit", Value::Null).await;
    language_server_client.wait_for_exit().await;
}

fn symbol_names(symbol_response: &Value) -> Vec<&str> {
    symbol_response["result"]
        .as_array()
        .expect("symbol result should be an array")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str())
        .collect()
}

fn feature_document_text() -> String {
    dsl! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4o"
        }

        schema Report {
            title: string
        }

        agent writer {
            model: model.openai_model
            instruction: "Write report"
            output {
                title: string
            }
        }

        output {
            report: agent.writer
        }
    }
}
