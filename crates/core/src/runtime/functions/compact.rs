use super::{BuiltinFunctionHandler, FunctionEvaluationRequest, FunctionTypeInferenceRequest};
use crate::dsl::Expression;
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::types::WorkflowType;
use serde_json::Value;

pub struct CompactFunction;

impl BuiltinFunctionHandler for CompactFunction {
    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowRuntimeError> {
        let Some(agent_reference_expression) = request.function_call.agent_argument_expression() else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) requires one agent reference argument".to_string(),
            });
        };

        let Expression::Reference(agent_reference) = agent_reference_expression else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) requires an `agent.<name>` reference".to_string(),
            });
        };

        if !agent_reference.is_agent_root() {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) only supports `agent.<name>` references".to_string(),
            });
        }

        let Some(agent_name) = agent_reference.first_access_field() else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) requires `agent.<name>` with a concrete agent name".to_string(),
            });
        };

        let Some(agent_context_value) = request.evaluation_context.agent_contexts.get(agent_name) else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: format!("context for agent `{agent_name}` is not available yet"),
            });
        };

        Ok(agent_context_value.clone())
    }

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowRuntimeError> {
        let compact_argument =
            request
                .function_call
                .agent_argument_expression()
                .ok_or_else(|| WorkflowRuntimeError::ExpressionEvaluation {
                    context: request.context.to_string(),
                    message: "compact(...) requires an agent reference argument".to_string(),
                })?;

        let _ = (request.infer_expression_type)(compact_argument, request.type_inference_context, request.context)?;

        Err(WorkflowRuntimeError::UnsupportedFeature {
            feature: "compact(...) is only supported in agent context expressions".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_inline_workflow;
    use crate::runtime::tests::{ScriptedRunner, BASE_PROVIDER_WORKFLOW};
    use crate::runtime::{WorkflowRuntime, WorkflowRuntimeError};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[tokio::test]
    async fn compact_function_uses_agent_context_for_context_property() {
        #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
        struct Output {
            final_message: String,
        }

        let workflow = parse_inline_workflow! {
            #BASE_PROVIDER_WORKFLOW;

            agent first {
                model: openai("model-a")
                prompt: "generate source"
                output: string
            }

            agent second {
                model: openai("model-a")
                context: compact(agent.first)
                prompt: "finalize"
                output: string
            }

            output {
                final_message: agent.second
            }
        };

        let runtime = WorkflowRuntime::<(), Output>::new(workflow).expect("runtime should compile");
        let runner = ScriptedRunner::from_outputs(vec![json!("source"), json!("done")]);

        let output = runtime
            .run_with_runner((), &runner)
            .await
            .expect("workflow should run successfully");

        assert_eq!(
            output,
            Output {
                final_message: "done".to_string(),
            }
        );

        let prompts = runner.prompts();
        let contexts = runner.contexts();

        assert_eq!(prompts[1], "finalize");

        assert_eq!(
            contexts[1],
            Some(json!({
                "agent": "first",
                "prompt": "generate source",
            }))
        );
    }

    #[tokio::test]
    async fn compact_function_rejects_non_agent_reference_argument() {
        #[derive(Debug, Serialize, JsonSchema)]
        struct Input {
            topic: String,
        }

        #[allow(dead_code)]
        #[derive(Debug, Deserialize, JsonSchema)]
        struct Output {
            final_message: String,
        }

        let workflow = parse_inline_workflow! {
            #BASE_PROVIDER_WORKFLOW;

            input {
                topic: string
            }

            agent second {
                model: openai("model-a")
                context: compact(input.topic)
                prompt: "finalize"
                output: string
            }

            output {
                final_message: agent.second
            }
        };

        let runtime = WorkflowRuntime::<Input, Output>::new(workflow).expect("runtime should compile");
        let runner = ScriptedRunner::from_outputs(vec![json!("done")]);

        let execution_result = runtime
            .run_with_runner(
                Input {
                    topic: "release".to_string(),
                },
                &runner,
            )
            .await;

        assert!(matches!(
            execution_result,
            Err(WorkflowRuntimeError::ExpressionEvaluation { message, .. })
                if message.contains("compact(...) only supports `agent.<name>` references")
        ));
    }
}
