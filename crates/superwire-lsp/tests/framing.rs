mod common;

use common::LspProcessClient;
use serde_json::{json, Value};

#[tokio::test]
async fn reads_multiple_framed_messages_from_single_input_batch() {
    let mut language_server_client = LspProcessClient::spawn();

    let initialize_request = json!({
        "jsonrpc": "2.0",
        "id": 101,
        "method": "initialize",
        "params": {
            "capabilities": {}
        }
    });

    let shutdown_request = json!({
        "jsonrpc": "2.0",
        "id": 102,
        "method": "shutdown",
        "params": null
    });

    let exit_notification = json!({
        "jsonrpc": "2.0",
        "method": "exit",
        "params": null
    });

    language_server_client
        .send_message_batch(&[initialize_request, shutdown_request, exit_notification])
        .await;

    let initialize_response = language_server_client.read_message().await;

    assert_eq!(initialize_response["jsonrpc"], "2.0");
    assert_eq!(initialize_response["id"], 101);
    assert!(initialize_response["result"]["capabilities"].is_object());

    let shutdown_response = language_server_client.read_message().await;

    assert_eq!(shutdown_response["jsonrpc"], "2.0");
    assert_eq!(shutdown_response["id"], 102);
    assert_eq!(shutdown_response["result"], Value::Null);

    language_server_client.wait_for_exit().await;
}
