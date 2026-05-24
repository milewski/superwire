#[macro_use]
mod support;

#[path = "negative/001_mcp_tool_output_schema_non_iterable_negative_test.rs"]
mod mcp_tool_output_schema_non_iterable_negative_test;

#[path = "negative/002_secrets_in_instruction_template_negative_test.rs"]
mod secrets_in_instruction_template_negative_test;

#[path = "negative/003_for_loop_agent_output_field_reference_negative_test.rs"]
mod for_loop_agent_output_field_reference_negative_test;
