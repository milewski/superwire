use crate::support::runner::TestRunner;
use serde_json::json;
use superwire_macros::workflow_source;

#[tokio::test]
async fn retries_failed_provider_request_and_succeeds() {
    let workflow = workflow_source! {
        provider openai from openai {
            api_key: "test-api-key"
        }

        model test_model from openai {
            id: "model-a"
        }

        agent analyst {
            model: model.test_model {
                inference {
                    provider_max_retries: 3
                    provider_retry_base_delay_ms: 10
                }
            }

            instruction: "Analyze the data."
            output {
                value: string
            }
        }

        output {
            analysis: agent.analyst.value
        }
    };

    let output = TestRunner::workflow(workflow)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .respond_error("temporary failure")
                    .turn()
                    .respond_json(json!({ "value": "success after retry" }));
            });
        })
        .run()
        .await
        .expect("workflow should succeed after retry");

    assert_eq!(output.output, json!({ "analysis": "success after retry" }));
}

#[tokio::test]
async fn retries_multiple_failures_before_succeeding() {
    let workflow = workflow_source! {
        provider openai from openai {
            api_key: "test-api-key"
        }

        model test_model from openai {
            id: "model-a"
        }

        agent analyst {
            model: model.test_model {
                inference {
                    provider_max_retries: 3
                    provider_retry_base_delay_ms: 10
                }
            }

            instruction: "Analyze the data."
            output {
                value: string
            }
        }

        output {
            analysis: agent.analyst.value
        }
    };

    let output = TestRunner::workflow(workflow)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .respond_error("failure 1")
                    .turn()
                    .respond_error("failure 2")
                    .turn()
                    .respond_error("failure 3")
                    .turn()
                    .respond_json(json!({ "value": "success after 3 retries" }));
            });
        })
        .run()
        .await
        .expect("workflow should succeed after 3 retries");

    assert_eq!(output.output, json!({ "analysis": "success after 3 retries" }));
}

#[tokio::test]
async fn fails_after_exhausting_all_retries() {
    let workflow = workflow_source! {
        provider openai from openai {
            api_key: "test-api-key"
        }

        model test_model from openai {
            id: "model-a"
        }

        agent analyst {
            model: model.test_model {
                inference {
                    provider_max_retries: 2
                    provider_retry_base_delay_ms: 10
                }
            }

            instruction: "Analyze the data."
            output {
                value: string
            }
        }

        output {
            analysis: agent.analyst.value
        }
    };

    let error = TestRunner::workflow(workflow)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .respond_error("failure 1")
                    .turn()
                    .respond_error("failure 2")
                    .turn()
                    .respond_error("failure 3");
            });
        })
        .run_expect_error()
        .await;

    let error_message = format!("{}", error.error);
    assert!(
        error_message.contains("failure 3"),
        "error should contain the last failure message, got: {error_message}"
    );
}

#[tokio::test]
async fn uses_default_retry_settings_when_not_configured() {
    let workflow = workflow_source! {
        provider openai from openai {
            api_key: "test-api-key"
        }

        model test_model from openai {
            id: "model-a"
        }

        agent analyst {
            model: model.test_model

            instruction: "Analyze the data."
            output {
                value: string
            }
        }

        output {
            analysis: agent.analyst.value
        }
    };

    let output = TestRunner::workflow(workflow)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model
                    .turn()
                    .respond_error("temporary failure")
                    .turn()
                    .respond_json(json!({ "value": "success" }));
            });
        })
        .run()
        .await
        .expect("workflow should succeed with default retry settings");

    assert_eq!(output.output, json!({ "analysis": "success" }));
}
