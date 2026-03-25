use crate::dsl::{BuiltinFunctionName, Expression, FunctionCall};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::EvaluationContext;
use crate::runtime::type_inference::TypeInferenceContext;
use crate::runtime::types::WorkflowType;
use serde_json::Value;

mod compact;
mod context;
mod template;

use compact::CompactFunction;
use context::ContextFunction;
use template::TemplateFunction;

pub type ExpressionEvaluator = dyn Fn(&Expression, &EvaluationContext, &str) -> Result<Value, WorkflowRuntimeError>;

pub type ExpressionTypeInferer = dyn Fn(&Expression, &TypeInferenceContext, &str) -> Result<WorkflowType, WorkflowRuntimeError>;

pub struct FunctionEvaluationRequest<'request> {
    pub function_call: &'request FunctionCall,
    pub evaluation_context: &'request EvaluationContext,
    pub context: &'request str,
    pub evaluate_expression: &'request ExpressionEvaluator,
}

pub struct FunctionTypeInferenceRequest<'request> {
    pub function_call: &'request FunctionCall,
    pub type_inference_context: &'request TypeInferenceContext,
    pub context: &'request str,
    pub infer_expression_type: &'request ExpressionTypeInferer,
}

pub trait BuiltinFunctionHandler {
    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowRuntimeError>;

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowRuntimeError>;
}

enum RegisteredBuiltinFunction {
    Context(ContextFunction),
    Template(TemplateFunction),
    Compact(CompactFunction),
}

impl RegisteredBuiltinFunction {
    fn from_name(function_name: BuiltinFunctionName) -> Self {
        match function_name {
            BuiltinFunctionName::Context => Self::Context(ContextFunction),
            BuiltinFunctionName::Template => Self::Template(TemplateFunction),
            BuiltinFunctionName::Compact => Self::Compact(CompactFunction),
        }
    }

    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowRuntimeError> {
        match self {
            Self::Context(function_handler) => function_handler.evaluate(request),
            Self::Template(function_handler) => function_handler.evaluate(request),
            Self::Compact(function_handler) => function_handler.evaluate(request),
        }
    }

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowRuntimeError> {
        match self {
            Self::Context(function_handler) => function_handler.infer_type(request),
            Self::Template(function_handler) => function_handler.infer_type(request),
            Self::Compact(function_handler) => function_handler.infer_type(request),
        }
    }
}

pub fn evaluate_builtin_function_call(
    function_call: &FunctionCall,
    evaluation_context: &EvaluationContext,
    context: &str,
    evaluate_expression: &ExpressionEvaluator,
) -> Result<Value, WorkflowRuntimeError> {
    let function_name = function_call
        .callee
        .root
        .as_identifier()
        .ok_or_else(|| WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: "function call must use identifier root".to_string(),
        })?;

    let Some(builtin_function_name) = BuiltinFunctionName::from_identifier(function_name) else {
        return Err(WorkflowRuntimeError::UnsupportedFeature {
            feature: format!("function `{function_name}` is not supported by runtime evaluator"),
        });
    };

    let function_request = FunctionEvaluationRequest {
        function_call,
        evaluation_context,
        context,
        evaluate_expression,
    };

    RegisteredBuiltinFunction::from_name(builtin_function_name).evaluate(&function_request)
}

pub fn infer_builtin_function_type(
    function_call: &FunctionCall,
    type_inference_context: &TypeInferenceContext,
    context: &str,
    infer_expression_type: &ExpressionTypeInferer,
) -> Result<WorkflowType, WorkflowRuntimeError> {
    let function_name = function_call
        .callee
        .root
        .as_identifier()
        .ok_or_else(|| WorkflowRuntimeError::ExpressionEvaluation {
            context: context.to_string(),
            message: "function call root must be an identifier".to_string(),
        })?;

    let Some(builtin_function_name) = BuiltinFunctionName::from_identifier(function_name) else {
        return Err(WorkflowRuntimeError::UnsupportedFeature {
            feature: format!("cannot infer return type for function `{function_name}`"),
        });
    };

    let function_request = FunctionTypeInferenceRequest {
        function_call,
        type_inference_context,
        context,
        infer_expression_type,
    };

    RegisteredBuiltinFunction::from_name(builtin_function_name).infer_type(&function_request)
}
