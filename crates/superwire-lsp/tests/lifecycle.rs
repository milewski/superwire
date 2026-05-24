#[macro_use]
mod common;

use common::{
    assert_publish_diagnostics_for_uri, did_change_params, did_close_params, did_open_params, text_document_position_params,
    LspProcessClient,
};
use serde_json::{json, Value};

#[tokio::test]
async fn routes_lifecycle_completion_and_hover_requests_over_stdio() {
    let mut language_server_client = LspProcessClient::spawn();
    let document_uri = "file:///workspace/workflow.engine";

    let initialize_response = language_server_client
        .send_request(1, "initialize", json!({ "capabilities": {} }))
        .await;

    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], 1);
    assert!(initialize_response["result"]["capabilities"]["completionProvider"].is_object());

    let initial_document_text = dsl! {
        input {
            first: string
        }

        agent helper {
            instruction: "Name: ${input.first}"
        }
    };

    language_server_client
        .send_notification("textDocument/didOpen", did_open_params(document_uri, &initial_document_text))
        .await;

    let open_diagnostics_notification = assert_publish_diagnostics_for_uri(&mut language_server_client, document_uri).await;

    assert!(open_diagnostics_notification["params"]["diagnostics"].is_array());

    let changed_document_text = dsl! {
        input {
            first: string
            last: string
        }

        agent helper {
            instruction: "Name: ${input.first}"
        }
    };

    language_server_client
        .send_notification("textDocument/didChange", did_change_params(document_uri, &changed_document_text))
        .await;

    let change_diagnostics_notification = assert_publish_diagnostics_for_uri(&mut language_server_client, document_uri).await;

    assert!(change_diagnostics_notification["params"]["diagnostics"].is_array());

    let completion_response = language_server_client
        .send_request(2, "textDocument/completion", text_document_position_params(document_uri, 6, 20))
        .await;

    assert_eq!(completion_response["jsonrpc"], "2.0");
    assert_eq!(completion_response["id"], 2);
    assert!(completion_response["result"]["items"].is_array());

    let hover_response = language_server_client
        .send_request(3, "textDocument/hover", text_document_position_params(document_uri, 1, 12))
        .await;

    assert_eq!(hover_response["jsonrpc"], "2.0");
    assert_eq!(hover_response["id"], 3);
    assert!(hover_response.get("result").is_some());

    language_server_client
        .send_notification("textDocument/didClose", did_close_params(document_uri))
        .await;

    let close_diagnostics_notification = assert_publish_diagnostics_for_uri(&mut language_server_client, document_uri).await;

    assert_eq!(close_diagnostics_notification["params"]["diagnostics"], json!([]));

    let shutdown_response = language_server_client.send_request(4, "shutdown", Value::Null).await;

    assert_eq!(shutdown_response["jsonrpc"], "2.0");
    assert_eq!(shutdown_response["id"], 4);
    assert_eq!(shutdown_response["result"], Value::Null);

    language_server_client.send_notification("exit", Value::Null).await;
    language_server_client.wait_for_exit().await;
}
