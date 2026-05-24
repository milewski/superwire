use super::{BuiltinFunctionHandler, FunctionEvaluationRequest, FunctionTypeInferenceRequest};
use crate::semantic::support::types::WorkflowType;
use crate::semantic::WorkflowSemanticError;
use serde_json::Value;
use superwire_types::ast::Expression;

pub struct CompactFunction;

impl BuiltinFunctionHandler for CompactFunction {
    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowSemanticError> {
        let Some(agent_reference_expression) = request.function_call.agent_argument_expression() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) requires one agent reference argument".to_string(),
            });
        };

        let Expression::Reference(agent_reference) = agent_reference_expression else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) requires an `agent.<name>` reference".to_string(),
            });
        };

        if !agent_reference.is_agent_root() {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) only supports `agent.<name>` references".to_string(),
            });
        }

        let Some(agent_name) = agent_reference.first_access_field() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "compact(...) requires `agent.<name>` with a concrete agent name".to_string(),
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
        let compact_argument =
            request
                .function_call
                .agent_argument_expression()
                .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                    context: request.context.to_string(),
                    message: "compact(...) requires an agent reference argument".to_string(),
                })?;

        let _ = (request.infer_expression_type)(compact_argument, request.type_inference_context, request.context)?;

        Err(WorkflowSemanticError::UnsupportedFeature {
            feature: "compact(...) is only supported in agent context expressions".to_string(),
        })
    }
}
