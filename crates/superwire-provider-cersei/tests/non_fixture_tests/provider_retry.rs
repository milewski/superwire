use crate::support::runner::TestRunner;
use serde_json::json;
use superwire_macros::workflow_source;
use superwire_protocol::event::{DiagnosticRetryability, ExecutorDiagnosticCode, ExecutorDiagnosticSubject, ExecutorEventKind};

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
    let attempt_events = output
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                ExecutorEventKind::ProviderAttemptStarted
                    | ExecutorEventKind::ProviderAttemptCompleted
                    | ExecutorEventKind::ProviderAttemptFailed
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(attempt_events.len(), 4);
    assert_eq!(attempt_events[0].kind, ExecutorEventKind::ProviderAttemptStarted);
    assert_eq!(attempt_events[1].kind, ExecutorEventKind::ProviderAttemptFailed);
    assert_eq!(attempt_events[2].kind, ExecutorEventKind::ProviderAttemptStarted);
    assert_eq!(attempt_events[3].kind, ExecutorEventKind::ProviderAttemptCompleted);
    assert_eq!(
        attempt_events[0].data.as_ref().and_then(|data| data.get("attempt")),
        Some(&json!(1))
    );
    assert_eq!(
        attempt_events[2].data.as_ref().and_then(|data| data.get("attempt")),
        Some(&json!(2))
    );
    assert!(attempt_events[3].data.as_ref().and_then(|data| data.get("duration_ms")).is_some());
    assert_eq!(
        attempt_events[1]
            .diagnostic
            .as_ref()
            .expect("failed attempt should include a diagnostic")
            .retryability,
        DiagnosticRetryability::Safe
    );
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

    let diagnostic = error.error.diagnostic();

    assert_eq!(diagnostic.code, ExecutorDiagnosticCode::ProviderRetriesExhausted);
    let final_attempt_diagnostic = diagnostic.cause.as_ref().expect("exhausted retries should retain the final cause");

    assert_eq!(final_attempt_diagnostic.message, "provider service failed");
    assert!(!final_attempt_diagnostic.message.contains("failure 3"));
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

#[tokio::test]
async fn does_not_retry_non_retryable_provider_status() {
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

    let error = TestRunner::workflow(workflow)
        .provider("openai", |provider| {
            provider.api_key("test-api-key").model("model-a", |model| {
                model.turn().respond_status_error(400, "invalid request");
            });
        })
        .run_expect_error()
        .await;
    let diagnostic = error.error.diagnostic();

    assert_eq!(error.provider_requests["openai"].len(), 1);
    assert_eq!(diagnostic.code, ExecutorDiagnosticCode::ModelProviderFailed);
    assert_eq!(diagnostic.retryability, DiagnosticRetryability::Never);

    let ExecutorDiagnosticSubject::Provider { attempt, http_status, .. } = diagnostic.subject else {
        panic!("provider failure should identify the provider attempt");
    };

    assert_eq!(attempt, Some(1));
    assert_eq!(http_status, Some(400));
}

#[tokio::test]
async fn rejects_retry_count_above_bound_before_provider_request() {
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
                    provider_max_retries: 9
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
            provider.api_key("test-api-key").model("model-a", |_| {});
        })
        .run_expect_error()
        .await;

    assert!(error.provider_requests["openai"].is_empty());
    assert_eq!(error.error.diagnostic().code, ExecutorDiagnosticCode::InvalidConfiguration);
}

#[tokio::test]
async fn rejects_retry_delay_above_bound_before_provider_request() {
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
                    provider_max_retries: 1
                    provider_retry_base_delay_ms: 60001
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
            provider.api_key("test-api-key").model("model-a", |_| {});
        })
        .run_expect_error()
        .await;

    assert!(error.provider_requests["openai"].is_empty());
    assert_eq!(error.error.diagnostic().code, ExecutorDiagnosticCode::InvalidConfiguration);
}
