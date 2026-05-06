use crate::api::ExecutionOptions;
use crate::service::ExecutorService;
use crate::tests::support::{request_with_input, ScriptedModelProvider, TestModelProvider};
use serde_json::{json, Value};
use superwire_core::workflow_source;

#[tokio::test]
async fn for_loop_over_literal_array() {
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        agent note for number in [1, 2, 3] {
            model: openai("model-a")
            prompt: "Write note for {{ number }}"
            output: {
                number: number
                note: string
            }
        }

        output {
            notes: agent.note
        }
    };
    let model_provider = ScriptedModelProvider::new(
        [(
            "note".to_string(),
            vec![
                json!({ "number": 1, "note": "first" }),
                json!({ "number": 2, "note": "second" }),
                json!({ "number": 3, "note": "third" }),
            ],
        )]
        .into(),
    );
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(workflow_source, Value::Null))
        .await
        .expect("for-loop over literal array should execute")
        .output;

    assert_eq!(
        output,
        json!({
            "notes": [
                { "number": 1, "note": "first" },
                { "number": 2, "note": "second" },
                { "number": 3, "note": "third" }
            ]
        })
    );
}

#[tokio::test]
async fn for_loop_over_input_array() {
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        input {
            items: [string]
        }

        agent processor for item in input.items {
            model: openai("model-a")
            prompt: "Process {{ item }}"
            output: string
        }

        output {
            results: agent.processor
        }
    };
    let model_provider = ScriptedModelProvider::new([("processor".to_string(), vec![json!("processed-a"), json!("processed-b")])].into());
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(workflow_source, json!({ "items": ["alpha", "beta"] })))
        .await
        .expect("for-loop over input array should execute")
        .output;

    assert_eq!(output, json!({ "results": ["processed-a", "processed-b"] }));
}

#[tokio::test]
async fn for_loop_with_object_destructuring() {
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        input {
            participants: [{
                id: number
                name: string
            }]
        }

        agent summarizer for { id, name } in input.participants {
            model: openai("model-a")
            prompt: "Summarize {{ id }} {{ name }}"
            output: string
        }

        output {
            summaries: agent.summarizer
        }
    };
    let model_provider =
        ScriptedModelProvider::new([("summarizer".to_string(), vec![json!("summary for Alice"), json!("summary for Bob")])].into());
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(
            workflow_source,
            json!({ "participants": [
                { "id": 1, "name": "Alice" },
                { "id": 2, "name": "Bob" }
            ] }),
        ))
        .await
        .expect("for-loop with object destructuring should execute")
        .output;

    assert_eq!(output, json!({ "summaries": ["summary for Alice", "summary for Bob"] }));
}

#[tokio::test]
async fn for_loop_empty_array_produces_empty_output() {
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        input {
            items: [string]
        }

        agent processor for item in input.items {
            model: openai("model-a")
            prompt: "Process {{ item }}"
            output: string
        }

        output {
            results: agent.processor
        }
    };
    let model_provider = TestModelProvider::new(vec![]);
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(workflow_source, json!({ "items": [] })))
        .await
        .expect("for-loop over empty array should succeed")
        .output;

    assert_eq!(output, json!({ "results": [] }));
}

#[tokio::test]
async fn for_loop_respects_max_concurrency() {
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        agent writer for number in [1, 2, 3, 4, 5] {
            model: openai("model-a")
            prompt: "Write {{ number }}"
            output: string
        }

        output {
            values: agent.writer
        }
    };
    let model_provider = ScriptedModelProvider::new(
        [(
            "writer".to_string(),
            vec![json!("a"), json!("b"), json!("c"), json!("d"), json!("e")],
        )]
        .into(),
    );
    let service = ExecutorService::new(model_provider);

    let mut request = crate::tests::support::request(workflow_source);
    request.options = ExecutionOptions {
        include_events: false,
        max_concurrency: 1,
    };

    let output = service
        .execute(request)
        .await
        .expect("for-loop with max_concurrency=1 should execute sequentially")
        .output;

    assert_eq!(output, json!({ "values": ["a", "b", "c", "d", "e"] }));
}

#[tokio::test]
async fn for_loop_can_reference_output_in_later_agent() {
    let workflow_source = workflow_source! {
        provider openai {
            driver: "openai"
            endpoint: "http://localhost:1234/v1"
            api_key: "test-api-key"
            models: ["model-a"]
        }

        agent scorer for item in [10, 20, 30] {
            model: openai("model-a")
            prompt: "Score {{ item }}"
            output: number
        }

        agent aggregator {
            model: openai("model-a")
            prompt: "Aggregate {{ agent.scorer }}"
            output: string
        }

        output {
            scores: agent.scorer
            aggregated: agent.aggregator
        }
    };
    let model_provider = ScriptedModelProvider::new(
        [
            ("scorer".to_string(), vec![json!(10), json!(20), json!(30)]),
            ("aggregator".to_string(), vec![json!("aggregated-60")]),
        ]
        .into(),
    );
    let service = ExecutorService::new(model_provider);

    let output = service
        .execute(request_with_input(workflow_source, Value::Null))
        .await
        .expect("for-loop output should be available to later agents")
        .output;

    assert_eq!(output["scores"], json!([10, 20, 30]));
    assert_eq!(output["aggregated"], "aggregated-60");
}
