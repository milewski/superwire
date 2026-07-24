use crate::McpError;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use superwire_semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_types::ast::{Declaration, Expression, McpServerDeclaration, McpServerPropertyName, Workflow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    pub name: String,
    pub endpoint: String,
    pub headers: BTreeMap<String, String>,
}

impl McpServerConfig {
    pub fn from_workflow(workflow: &Workflow) -> Result<Vec<Self>, McpError> {
        Ok(workflow
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                Declaration::McpServer(mcp_server_declaration) => Self::from_declaration(mcp_server_declaration),
                Declaration::Provider(_)
                | Declaration::Model(_)
                | Declaration::Secrets(_)
                | Declaration::Input(_)
                | Declaration::Schema(_)
                | Declaration::Tool(_)
                | Declaration::McpBatch(_)
                | Declaration::McpToolBatch(_)
                | Declaration::McpResourceBatch(_)
                | Declaration::McpPromptBatch(_)
                | Declaration::McpResource(_)
                | Declaration::McpPrompt(_)
                | Declaration::Dynamic(_)
                | Declaration::Agent(_)
                | Declaration::Output(_) => None,
            })
            .collect())
    }

    #[must_use]
    pub fn from_declaration(mcp_server_declaration: &McpServerDeclaration) -> Option<Self> {
        let server_name = mcp_server_declaration.name.clone();
        let mut endpoint = None;
        let mut headers = BTreeMap::new();

        for property in &mcp_server_declaration.properties {
            match McpServerPropertyName::from_identifier(&property.name) {
                Some(McpServerPropertyName::Endpoint) => {
                    let Expression::StringLiteral(value) = &property.value else {
                        return None;
                    };
                    endpoint = Some(value.clone());
                }
                Some(McpServerPropertyName::Headers) => {
                    headers = Self::parse_literal_headers(&property.value)?;
                }
                None => {}
            }
        }

        let endpoint = endpoint?;

        Some(Self {
            name: server_name,
            endpoint,
            headers,
        })
    }

    pub fn resolve_from_declaration(
        mcp_server_declaration: &McpServerDeclaration,
        evaluation_context: &EvaluationContext,
    ) -> Result<Self, McpError> {
        Self::resolve_from_declaration_with_endpoint_validator(mcp_server_declaration, evaluation_context, |_server_name, _endpoint| Ok(()))
    }

    pub fn resolve_endpoint_from_declaration(
        mcp_server_declaration: &McpServerDeclaration,
        evaluation_context: &EvaluationContext,
    ) -> Result<(String, String), McpError> {
        let server_name = mcp_server_declaration.name.clone();
        let endpoint_property_name = McpServerPropertyName::Endpoint.as_str();
        let mut endpoint = None;

        for property in &mcp_server_declaration.properties {
            if McpServerPropertyName::from_identifier(&property.name) != Some(McpServerPropertyName::Endpoint) {
                continue;
            }

            let value = evaluate_expression(
                &property.value,
                evaluation_context,
                &format!("MCP server `{server_name}` property `{endpoint_property_name}`"),
            )
            .map_err(|error| McpError::InvalidPropertyEvaluation {
                server_name: server_name.clone(),
                property_name: endpoint_property_name.to_string(),
                reason: error.to_string(),
            })?;
            let string_value = value.as_str().ok_or_else(|| McpError::InvalidProperty {
                server_name: server_name.clone(),
                property_name: endpoint_property_name.to_string(),
                expected: "a string value",
            })?;
            endpoint = Some(string_value.to_string());
        }

        let endpoint = endpoint.ok_or_else(|| McpError::MissingEndpoint {
            server_name: server_name.clone(),
        })?;

        Ok((server_name, endpoint))
    }

    pub fn resolve_from_declaration_with_endpoint_validator<EndpointValidator>(
        mcp_server_declaration: &McpServerDeclaration,
        evaluation_context: &EvaluationContext,
        validate_endpoint: EndpointValidator,
    ) -> Result<Self, McpError>
    where
        EndpointValidator: FnOnce(&str, &str) -> Result<(), McpError>,
    {
        let (server_name, endpoint) = Self::resolve_endpoint_from_declaration(mcp_server_declaration, evaluation_context)?;
        validate_endpoint(&server_name, &endpoint)?;
        let mut headers = BTreeMap::new();

        for property in &mcp_server_declaration.properties {
            if McpServerPropertyName::from_identifier(&property.name) == Some(McpServerPropertyName::Headers) {
                headers = Self::resolve_headers(&property.value, &server_name, evaluation_context)?;
            }
        }

        Ok(Self {
            name: server_name,
            endpoint,
            headers,
        })
    }

    fn parse_literal_headers(expression: &Expression) -> Option<BTreeMap<String, String>> {
        let Expression::ObjectLiteral(header_fields) = expression else {
            return None;
        };

        let mut headers = BTreeMap::new();

        for header_field in header_fields {
            let Expression::StringLiteral(value) = &header_field.value else {
                return None;
            };
            headers.insert(header_field.name.clone(), value.clone());
        }

        Some(headers)
    }

    fn resolve_headers(
        expression: &Expression,
        server_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<BTreeMap<String, String>, McpError> {
        let Expression::ObjectLiteral(header_fields) = expression else {
            return Err(McpError::InvalidProperty {
                server_name: server_name.to_string(),
                property_name: "headers".to_string(),
                expected: "an object with string values",
            });
        };

        let mut headers = BTreeMap::new();

        for header_field in header_fields {
            let value = evaluate_expression(
                &header_field.value,
                evaluation_context,
                &format!("MCP server `{server_name}` header `{}`", header_field.name),
            )
            .map_err(|error| McpError::InvalidPropertyEvaluation {
                server_name: server_name.to_string(),
                property_name: format!("headers.{}", header_field.name),
                reason: error.to_string(),
            })?;
            let string_value = value.as_str().ok_or_else(|| McpError::InvalidProperty {
                server_name: server_name.to_string(),
                property_name: format!("headers.{}", header_field.name),
                expected: "a string value",
            })?;
            headers.insert(header_field.name.clone(), string_value.to_string());
        }

        Ok(headers)
    }
}

impl Display for McpServerConfig {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({})", self.name, self.endpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpMcpClientFactory, McpClientFactory};
    use std::collections::HashMap;
    use superwire_types::ast::{ObjectField, Reference, ReferenceAccess, ReferenceKeyword, ReferenceRoot, SourcePosition, SourceSpan};

    #[test]
    fn endpoint_policy_runs_before_header_values_are_materialized() {
        let source_position = SourcePosition { line: 1, column: 1 };
        let source_span = SourceSpan {
            start: source_position,
            end: source_position,
        };
        let mcp_server_declaration = McpServerDeclaration {
            name: "blocked".to_string(),
            properties: vec![
                ObjectField {
                    name: McpServerPropertyName::Headers.as_str().to_string(),
                    value: Expression::ObjectLiteral(vec![ObjectField {
                        name: "Authorization".to_string(),
                        value: Expression::Reference(Reference {
                            root: ReferenceRoot::Keyword(ReferenceKeyword::Secrets),
                            accesses: vec![ReferenceAccess::required("mcp_token")],
                            span: source_span,
                        }),
                        span: source_span,
                    }]),
                    span: source_span,
                },
                ObjectField {
                    name: McpServerPropertyName::Endpoint.as_str().to_string(),
                    value: Expression::StringLiteral("http://127.0.0.1:3000/mcp".to_string()),
                    span: source_span,
                },
            ],
            span: source_span,
        };
        let evaluation_context = EvaluationContext {
            input_values: serde_json::Map::new(),
            secret_values: serde_json::Map::new(),
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
        };
        let client_factory = HttpMcpClientFactory;
        let error = McpServerConfig::resolve_from_declaration_with_endpoint_validator(
            &mcp_server_declaration,
            &evaluation_context,
            |server_name, endpoint| client_factory.validate_endpoint(server_name, endpoint),
        )
        .expect_err("disabled policy should reject before the invalid header value is evaluated");

        assert!(error.is_network_policy_violation());
    }
}
