use super::{BuiltinFunctionHandler, FunctionEvaluationRequest, FunctionTypeInferenceRequest};
use crate::dsl::{BuiltinFunctionArgumentName, CallArgument, Expression, ReferenceKeyword};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::types::WorkflowType;
use serde_json::Value;

pub struct ContextFunction;

impl BuiltinFunctionHandler for ContextFunction {
    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowRuntimeError> {
        let Some(agent_reference_expression) = context_call_agent_argument(request.function_call) else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires one agent reference argument".to_string(),
            });
        };

        let Expression::Reference(agent_reference) = agent_reference_expression else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires an `agent.<name>` reference".to_string(),
            });
        };

        if agent_reference.root.keyword() != Some(ReferenceKeyword::Agent) {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) only supports `agent.<name>` references".to_string(),
            });
        }

        let Some(agent_name_access) = agent_reference.accesses.first() else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires `agent.<name>` with a concrete agent name".to_string(),
            });
        };

        let Some(agent_context_value) = request.evaluation_context.agent_contexts.get(&agent_name_access.field) else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: format!("context for agent `{}` is not available yet", agent_name_access.field),
            });
        };

        Ok(agent_context_value.clone())
    }

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowRuntimeError> {
        let _ = context_call_agent_argument(request.function_call)
            .ok_or_else(|| WorkflowRuntimeError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires an agent reference argument".to_string(),
            })
            .and_then(|argument_expression| {
                (request.infer_expression_type)(argument_expression, request.type_inference_context, request.context)
            })?;

        Err(WorkflowRuntimeError::UnsupportedFeature {
            feature: "context(...) is not supported in statically typed output values".to_string(),
        })
    }
}

fn context_call_agent_argument(function_call: &crate::dsl::FunctionCall) -> Option<&Expression> {
    for call_argument in &function_call.arguments {
        match call_argument {
            CallArgument::Positional(expression) => {
                return Some(expression);
            }
            CallArgument::Named(named_argument) => {
                if BuiltinFunctionArgumentName::from_identifier(named_argument.name.as_str()) == Some(BuiltinFunctionArgumentName::Agent) {
                    return Some(&named_argument.value);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use crate::parse_inline_workflow;
    use crate::runtime::tests::{ScriptedRunner, BASE_PROVIDER_WORKFLOW};
    use crate::runtime::WorkflowRuntime;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;

    #[tokio::test]
    async fn context_function_uses_agent_context_for_context_property() {
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
                context: context(agent.first)
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

        assert!(prompts[1].contains("Context:"));
        assert!(prompts[1].contains("\"prompt\": \"generate source\""));
    }
}
