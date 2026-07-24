use serde_json::json;
use superwire_executor::runtime::WorkflowExecutor;
use superwire_macros::workflow_source;
use superwire_provider_cersei::{CerseiModelProvider, ProviderNetworkPolicy};

#[tokio::test]
#[ignore = "requires SUPERWIRE_SMOKE_OPENAI_ENDPOINT, SUPERWIRE_SMOKE_OPENAI_API_KEY, and SUPERWIRE_SMOKE_OPENAI_MODEL"]
async fn executes_real_openai_compatible_workflow_from_environment() {
    let endpoint = std::env::var("SUPERWIRE_SMOKE_OPENAI_ENDPOINT")
        .expect("SUPERWIRE_SMOKE_OPENAI_ENDPOINT must be set when running the live smoke test");
    let api_key = std::env::var("SUPERWIRE_SMOKE_OPENAI_API_KEY")
        .expect("SUPERWIRE_SMOKE_OPENAI_API_KEY must be set when running the live smoke test");
    let model =
        std::env::var("SUPERWIRE_SMOKE_OPENAI_MODEL").expect("SUPERWIRE_SMOKE_OPENAI_MODEL must be set when running the live smoke test");
    let workflow = workflow_source! {
        secrets {
            endpoint: string
            api_key: string
            model: string
        }

        provider smoke from openai_compatible {
            endpoint: secrets.endpoint
            api_key: secrets.api_key
        }

        model smoke_model from smoke {
            id: secrets.model
        }

        agent responder {
            model: model.smoke_model
            instruction: "Return a short greeting for the Superwire live smoke test."
            output {
                greeting: string
            }
        }

        output {
            greeting: agent.responder.greeting
        }
    };
    let secrets = json!({
        "endpoint": endpoint,
        "api_key": api_key,
        "model": model,
    });
    let executor = WorkflowExecutor::from_source_with_runtime_values(workflow, &serde_json::Value::Null, &secrets)
        .expect("live smoke workflow should build");

    let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);

    let output = executor
        .execute(serde_json::Value::Null, secrets, &model_provider, None, 1)
        .await
        .expect("live OpenAI-compatible workflow should complete");
    let greeting = output
        .get("greeting")
        .and_then(serde_json::Value::as_str)
        .expect("live workflow should return a string greeting");

    assert!(!greeting.trim().is_empty());
}
