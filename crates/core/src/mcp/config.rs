use crate::dsl::{Declaration, Expression, McpServerDeclaration, McpServerPropertyName, Workflow};
use crate::mcp::McpError;
use crate::semantic::support::expression::{evaluate_expression, EvaluationContext};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

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
                | Declaration::Secrets(_)
                | Declaration::Input(_)
                | Declaration::Schema(_)
                | Declaration::Tool(_)
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
        let server_name = mcp_server_declaration.name.clone();
        let mut endpoint = None;
        let mut headers = BTreeMap::new();

        for property in &mcp_server_declaration.properties {
            match McpServerPropertyName::from_identifier(&property.name) {
                Some(McpServerPropertyName::Endpoint) => {
                    let value = evaluate_expression(
                        &property.value,
                        evaluation_context,
                        &format!("MCP server `{server_name}` property `endpoint`"),
                    )
                    .map_err(|_error| McpError::InvalidProperty {
                        server_name: server_name.clone(),
                        property_name: "endpoint".to_string(),
                        expected: "a string or reference that resolves to a string",
                    })?;
                    let string_value = value.as_str().ok_or_else(|| McpError::InvalidProperty {
                        server_name: server_name.clone(),
                        property_name: "endpoint".to_string(),
                        expected: "a string value",
                    })?;
                    endpoint = Some(string_value.to_string());
                }
                Some(McpServerPropertyName::Headers) => {
                    headers = Self::resolve_headers(&property.value, &server_name, evaluation_context)?;
                }
                None => {}
            }
        }

        let endpoint = endpoint.ok_or_else(|| McpError::MissingEndpoint {
            server_name: server_name.clone(),
        })?;

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
            .map_err(|_error| McpError::InvalidProperty {
                server_name: server_name.to_string(),
                property_name: format!("headers.{}", header_field.name),
                expected: "a string or reference that resolves to a string",
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
