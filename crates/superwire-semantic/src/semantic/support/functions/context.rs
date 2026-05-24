use super::{BuiltinFunctionHandler, FunctionEvaluationRequest, FunctionTypeInferenceRequest};
use crate::semantic::support::types::WorkflowType;
use crate::semantic::WorkflowSemanticError;
use serde_json::Value;
use superwire_types::ast::Expression;

pub struct ContextFunction;

impl BuiltinFunctionHandler for ContextFunction {
    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowSemanticError> {
        let Some(agent_reference_expression) = request.function_call.agent_argument_expression() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires one agent reference argument".to_string(),
            });
        };

        let Expression::Reference(agent_reference) = agent_reference_expression else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires an `agent.<name>` reference".to_string(),
            });
        };

        if !agent_reference.is_agent_root() {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) only supports `agent.<name>` references".to_string(),
            });
        }

        let Some(agent_name) = agent_reference.first_access_field() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires `agent.<name>` with a concrete agent name".to_string(),
            });
        };

        let Some(agent_context_value) = request.evaluation_context.agent_contexts.get(agent_name) else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: format!("context for agent `{agent_name}` is not available yet"),
            });
        };

        Ok(agent_context_value.clone())
    }

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowSemanticError> {
        let _ = request
            .function_call
            .agent_argument_expression()
            .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "context(...) requires an agent reference argument".to_string(),
            })
            .and_then(|argument_expression| {
                (request.infer_expression_type)(argument_expression, request.type_inference_context, request.context)
            })?;

        Err(WorkflowSemanticError::UnsupportedFeature {
            feature: "context(...) is not supported in statically typed output values".to_string(),
        })
    }
}
