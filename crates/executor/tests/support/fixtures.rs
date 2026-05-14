pub const MINIMUM: &str = include_str!("../fixtures/001_minimum.wire");
pub const STRING_OUTPUT: &str = include_str!("../fixtures/002_string_output.wire");
pub const OBJECT_OUTPUT: &str = include_str!("../fixtures/003_object_output.wire");
pub const LINEAR_CHAIN: &str = include_str!("../fixtures/004_linear_chain.wire");
pub const PARALLEL_AGENTS: &str = include_str!("../fixtures/005_parallel_agents.wire");
pub const INPUT_STRING: &str = include_str!("../fixtures/006_input_string.wire");
pub const INPUT_OBJECT: &str = include_str!("../fixtures/007_input_object.wire");
pub const INPUT_ARRAY: &str = include_str!("../fixtures/008_input_array.wire");
pub const SECRETS: &str = include_str!("../fixtures/009_secrets.wire");
pub const DYNAMIC_VALUES: &str = include_str!("../fixtures/010_dynamic_values.wire");
pub const STRING_INTERPOLATION: &str = include_str!("../fixtures/011_string_interpolation.wire");
pub const NESTED_OUTPUT: &str = include_str!("../fixtures/012_nested_output.wire");
pub const HARDCODED_OUTPUT: &str = include_str!("../fixtures/013_hardcoded_output.wire");
pub const MULTILINE_PROMPT: &str = include_str!("../fixtures/014_multiline_prompt.wire");
pub const INFERENCE_SETTINGS: &str = include_str!("../fixtures/015_inference_settings.wire");
pub const SCHEMA_OUTPUT: &str = include_str!("../fixtures/016_schema_output.wire");
pub const COMPLEX_TYPES: &str = include_str!("../fixtures/017_complex_types.wire");
pub const OPTIONAL_CHAINING: &str = include_str!("../fixtures/018_optional_chaining.wire");
pub const DIAMOND_DEPENDENCY: &str = include_str!("../fixtures/019_diamond_dependency.wire");
pub const MIXED_OUTPUT: &str = include_str!("../fixtures/020_mixed_output.wire");
pub const DYNAMIC_TOOL_CALL: &str = include_str!("../fixtures/021_dynamic_tool_call.wire");
pub const MCP_READ_RESOURCE: &str = include_str!("../fixtures/022_mcp_read_resource.wire");
pub const MCP_RENDER_PROMPT: &str = include_str!("../fixtures/023_mcp_render_prompt.wire");
pub const MCP_READ_RENDER_DEPENDENCIES: &str = include_str!("../fixtures/024_mcp_read_render_dependencies.wire");
pub const TOOL_MAX_CALLS_SCOPES: &str = include_str!("../fixtures/025_tool_max_calls_scopes.wire");
pub const MCP_TOOL_BATCH_IMPORTS: &str = include_str!("../fixtures/026_mcp_tool_batch_imports.wire");
pub const AGENT_FINALIZE_TOOL: &str = include_str!("../fixtures/027_agent_finalize_tool.wire");
pub const MCP_PROMPT_RESOURCE_BATCH_IMPORTS: &str = include_str!("../fixtures/028_mcp_prompt_resource_batch_imports.wire");
pub const MCP_MIXED_BATCH_IMPORTS: &str = include_str!("../fixtures/029_mcp_mixed_batch_imports.wire");
pub const MCP_TOOL_OUTPUT_ITERABLE_TYPE_MISMATCH: &str = include_str!("../fixtures/030_mcp_tool_output_iterable_type_mismatch.wire");
pub const MULTIPLE_PROVIDERS_MODELS: &str = include_str!("../fixtures/031_multiple_providers_models.wire");
pub const AGENT_LOCAL_DYNAMIC_TOOL_SCOPE: &str = include_str!("../fixtures/032_agent_local_dynamic_tool_scope.wire");
pub const AGENT_LOCAL_DYNAMIC_BINDINGS_OVERRIDE_APPEND: &str =
    include_str!("../fixtures/033_agent_local_dynamic_bindings_override_append.wire");
pub const MCP_TOOL_OUTPUT_SCHEMA_OVERRIDE: &str = include_str!("../fixtures/034_mcp_tool_output_schema_override.wire");
pub const MCP_PROMPT_REQUIRED_BINDING_VALIDATION: &str = include_str!("../fixtures/035_mcp_prompt_required_binding_validation.wire");
pub const VARIANT_MATCH_PROJECTION: &str = include_str!("../fixtures/036_variant_match_projection.wire");
pub const SCHEMA_TYPES: &str = include_str!("../fixtures/037_schema_types.wire");
pub const SCHEMA_VARIANT_TYPES: &str = include_str!("../fixtures/038_schema_variant_types.wire");

// Negative Tests
pub const MCP_TOOL_OUTPUT_SCHEMA_OVERRIDE_NON_ITERABLE: &str =
    include_str!("../fixtures/negative/001_mcp_tool_output_schema_non_iterable_negative.wire");
pub const SECRETS_IN_INSTRUCTION_TEMPLATE: &str = include_str!("../fixtures/negative/002_secrets_in_instruction_template_negative.wire");
