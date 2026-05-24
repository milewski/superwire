use crate::semantic::support::expression::EvaluationContext;
use crate::semantic::support::type_inference::TypeInferenceContext;
use crate::semantic::support::types::WorkflowType;
use crate::semantic::WorkflowSemanticError;
use serde_json::Value;
use superwire_types::ast::{BuiltinFunctionName, Expression, FunctionCall};

mod compact;
mod context;
mod template;

use compact::CompactFunction;
use context::ContextFunction;
use template::TemplateFunction;

pub type ExpressionEvaluator = dyn Fn(&Expression, &EvaluationContext, &str) -> Result<Value, WorkflowSemanticError>;

pub type ExpressionTypeInferer = dyn Fn(&Expression, &TypeInferenceContext, &str) -> Result<WorkflowType, WorkflowSemanticError>;

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
    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowSemanticError>;

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowSemanticError>;
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

    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowSemanticError> {
        match self {
            Self::Context(function_handler) => function_handler.evaluate(request),
            Self::Template(function_handler) => function_handler.evaluate(request),
            Self::Compact(function_handler) => function_handler.evaluate(request),
        }
    }

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowSemanticError> {
        match self {
            Self::Context(function_handler) => function_handler.infer_type(request),
            Self::Template(function_handler) => function_handler.infer_type(request),
            Self::Compact(function_handler) => function_handler.infer_type(request),
        }
    }
}

pub trait FunctionCallSemanticExt {
    fn evaluate_builtin(
        &self,
        evaluation_context: &EvaluationContext,
        context: &str,
        evaluate_expression: &ExpressionEvaluator,
    ) -> Result<Value, WorkflowSemanticError>;
    fn infer_builtin_type(
        &self,
        type_inference_context: &TypeInferenceContext,
        context: &str,
        infer_expression_type: &ExpressionTypeInferer,
    ) -> Result<WorkflowType, WorkflowSemanticError>;
}

impl FunctionCallSemanticExt for FunctionCall {
    fn evaluate_builtin(
        &self,
        evaluation_context: &EvaluationContext,
        context: &str,
        evaluate_expression: &ExpressionEvaluator,
    ) -> Result<Value, WorkflowSemanticError> {
        let function_name = self.identifier_name().ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: "function call must use identifier root".to_string(),
        })?;

        let Some(builtin_function_name) = self.builtin_function_name() else {
            return Err(WorkflowSemanticError::UnsupportedFeature {
                feature: format!("function `{function_name}` is not supported by runtime evaluator"),
            });
        };

        let function_request = FunctionEvaluationRequest {
            function_call: self,
            evaluation_context,
            context,
            evaluate_expression,
        };

        RegisteredBuiltinFunction::from_name(builtin_function_name).evaluate(&function_request)
    }

    fn infer_builtin_type(
        &self,
        type_inference_context: &TypeInferenceContext,
        context: &str,
        infer_expression_type: &ExpressionTypeInferer,
    ) -> Result<WorkflowType, WorkflowSemanticError> {
        let function_name = self.identifier_name().ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: "function call root must be an identifier".to_string(),
        })?;

        let Some(builtin_function_name) = self.builtin_function_name() else {
            return Err(WorkflowSemanticError::UnsupportedFeature {
                feature: format!("cannot infer return type for function `{function_name}`"),
            });
        };

        let function_request = FunctionTypeInferenceRequest {
            function_call: self,
            type_inference_context,
            context,
            infer_expression_type,
        };

        RegisteredBuiltinFunction::from_name(builtin_function_name).infer_type(&function_request)
    }
}
