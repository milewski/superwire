<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution\Compiler;

use JsonException;
use Superwire\Laravel\Contracts\Tool;

final readonly class ToolModuleSourceGenerator
{
    public function __construct(
        private ToolNameFormatter $toolNameFormatter,
        private ToolEndpointResolver $toolEndpointResolver,
        private ToolSchemaPayloadSerializer $toolSchemaPayloadSerializer,
        private ToolModuleTemplateRenderer $toolModuleTemplateRenderer,
    )
    {
    }

    /**
     * @param class-string<Tool> $toolClass
     * @throws JsonException
     */
    public function generate(string $toolClass): string
    {
        $toolName = $toolClass::name();
        $toolTypeName = $this->toolNameFormatter->typeName($toolName);
        $agentInputTypeName = sprintf('%sAgentInput', $toolTypeName);
        $boundInputTypeName = sprintf('%sBoundInput', $toolTypeName);
        $outputTypeName = sprintf('%sOutput', $toolTypeName);

        return $this->toolModuleTemplateRenderer->render([
            'agent_input_type_name' => $agentInputTypeName,
            'agent_input_schema_json' => $this->toolSchemaPayloadSerializer->escapedJsonString($toolClass::inputSchema()),
            'bound_input_type_name' => $boundInputTypeName,
            'bound_input_schema_json' => $this->toolSchemaPayloadSerializer->escapedJsonString($toolClass::boundInputSchema()),
            'output_type_name' => $outputTypeName,
            'output_schema_json' => $this->toolSchemaPayloadSerializer->escapedJsonString($toolClass::outputSchema()),
            'tool_type_name' => $toolTypeName,
            'tool_name' => $this->toolSchemaPayloadSerializer->escapedString($toolName),
            'tool_description' => $this->toolSchemaPayloadSerializer->escapedString($toolClass::description()),
            'tool_endpoint' => $this->toolSchemaPayloadSerializer->escapedString(
                $this->toolEndpointResolver->resolve($toolClass::endpointName()),
            ),
        ]);
    }
}
