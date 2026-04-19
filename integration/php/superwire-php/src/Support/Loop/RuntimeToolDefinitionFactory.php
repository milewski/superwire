<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Loop;

use Superwire\Contracts\Agent\AgentToolDefinition;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolMetadataProviderInterface;
use Superwire\Contracts\Contracts\RuntimeToolSchemaProviderInterface;
use Swaggest\JsonSchema\Schema;

final readonly class RuntimeToolDefinitionFactory
{
    public function __construct(
        private ?RuntimeToolInvokerInterface $runtimeToolInvoker,
    )
    {
    }

    public function definitionForToolName(string $toolName): AgentToolDefinition
    {
        return new AgentToolDefinition(
            name: $toolName,
            description: $this->runtimeToolDescription($toolName),
            parametersSchema: $this->runtimeToolParametersSchema($toolName),
            strict: $this->runtimeToolStrictMode($toolName),
        );
    }

    private function runtimeToolParametersSchema(string $toolName): Schema
    {
        if ($this->runtimeToolInvoker instanceof RuntimeToolSchemaProviderInterface) {

            $toolSchema = $this->runtimeToolInvoker->schemaForTool($toolName);

            if ($toolSchema instanceof Schema) {
                return $toolSchema;
            }

        }

        return Schema::object()
            ->setProperties((object) [])
            ->setAdditionalProperties(false);
    }

    private function runtimeToolDescription(string $toolName): string
    {
        if ($this->runtimeToolInvoker instanceof RuntimeToolMetadataProviderInterface) {

            $toolDescription = $this->runtimeToolInvoker->descriptionForTool($toolName);

            if (is_string($toolDescription) && $toolDescription !== '') {
                return $toolDescription;
            }

        }

        return "Execute runtime tool `{$toolName}` and use result to continue";
    }

    private function runtimeToolStrictMode(string $toolName): bool
    {
        if ($this->runtimeToolInvoker instanceof RuntimeToolMetadataProviderInterface) {

            $strictMode = $this->runtimeToolInvoker->strictSchemaForTool($toolName);

            if (is_bool($strictMode)) {
                return $strictMode;
            }

        }

        return true;
    }
}
