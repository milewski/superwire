<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Workflow;

use Superwire\Contracts\Agent\AgentDefinition;
use Superwire\Contracts\Agent\AgentForEachDefinition;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Provider\ProviderDefinition;

final class WorkflowDefinition
{
    /**
     * @param array<string, mixed>|null $input
     * @param array<string, mixed>|null $secrets
     * @param list<array{name: string, fields: list<array<string, mixed>>}> $schemas
     * @param list<ProviderDefinition> $providers
     * @param list<AgentDefinition> $agents
     * @param array<string, mixed> $output
     * @param array<string, mixed> $execution
     */
    public function __construct(
        public readonly string $format,
        public readonly string $workflowPath,
        public readonly ?array $input,
        public readonly ?array $secrets,
        public readonly array $schemas,
        public readonly array $providers,
        public readonly array $agents,
        public readonly array $output,
        public readonly array $execution,
    ) {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromArray(array $payload): self
    {
        $format = self::requiredString($payload, 'format');
        $workflowPath = self::requiredString($payload, 'workflow_path');
        $providersPayload = self::requiredList($payload, 'providers');
        $agentsPayload = self::requiredList($payload, 'agents');
        $outputPayload = self::requiredArray($payload, 'output');
        $executionPayload = self::requiredArray($payload, 'execution');
        $inputPayload = self::optionalArray($payload, 'input');
        $secretsPayload = self::optionalArray($payload, 'secrets');
        $schemasPayload = self::optionalList($payload, 'schemas');

        $providers = [];

        foreach ($providersPayload as $providerPayload) {

            if (!is_array($providerPayload)) {
                throw new InvalidWorkflowDefinitionException('provider entries must be objects');
            }

            $providerName = self::requiredString($providerPayload, 'name');
            $driverName = self::requiredString($providerPayload, 'driver');
            $providerConfig = self::requiredArray($providerPayload, 'config');
            $providerModels = $providerPayload[ 'models' ] ?? null;

            $providers[] = new ProviderDefinition($providerName, $driverName, $providerConfig, $providerModels);

        }

        $agents = [];

        foreach ($agentsPayload as $agentPayload) {

            if (!is_array($agentPayload)) {
                throw new InvalidWorkflowDefinitionException('agent entries must be objects');
            }

            $agentName = self::requiredString($agentPayload, 'name');
            $providerName = self::requiredString($agentPayload, 'provider');
            $modelValue = self::requiredValue($agentPayload, 'model');
            $promptValue = self::requiredValue($agentPayload, 'prompt');
            $outputValue = self::requiredValue($agentPayload, 'output');
            $batchIndex = self::requiredInteger($agentPayload, 'batch');
            $contextValue = $agentPayload[ 'context' ] ?? null;
            $inferenceValue = $agentPayload[ 'inference' ] ?? null;
            $dependencies = self::optionalStringList($agentPayload, 'dependencies');
            $dependents = self::optionalStringList($agentPayload, 'dependents');
            $tools = self::optionalToolBindings($agentPayload);
            $forEach = null;

            if (array_key_exists('for_each', $agentPayload) && $agentPayload[ 'for_each' ] !== null) {

                if (!is_array($agentPayload[ 'for_each' ])) {
                    throw new InvalidWorkflowDefinitionException('agent for_each must be an object when present');
                }

                $forEachPattern = self::requiredArray($agentPayload[ 'for_each' ], 'pattern');
                $forEachIterable = self::requiredValue($agentPayload[ 'for_each' ], 'iterable');
                $forEach = new AgentForEachDefinition($forEachPattern, $forEachIterable);

            }

            $agents[] = new AgentDefinition(
                name: $agentName,
                provider: $providerName,
                model: $modelValue,
                prompt: $promptValue,
                context: $contextValue,
                inference: $inferenceValue,
                tools: $tools,
                forEach: $forEach,
                output: $outputValue,
                dependencies: $dependencies,
                dependents: $dependents,
                batch: $batchIndex,
            );

        }

        return new self(
            format: $format,
            workflowPath: $workflowPath,
            input: $inputPayload,
            secrets: $secretsPayload,
            schemas: $schemasPayload,
            providers: $providers,
            agents: $agents,
            output: $outputPayload,
            execution: $executionPayload,
        );
    }

    public function providerByName(string $providerName): ?ProviderDefinition
    {
        foreach ($this->providers as $providerDefinition) {

            if ($providerDefinition->name === $providerName) {
                return $providerDefinition;
            }

        }

        return null;
    }

    public function agentByName(string $agentName): ?AgentDefinition
    {
        foreach ($this->agents as $agentDefinition) {

            if ($agentDefinition->name === $agentName) {
                return $agentDefinition;
            }

        }

        return null;
    }

    /**
     * @param array<string, mixed> $payload
     */
    private static function requiredString(array $payload, string $key): string
    {
        if (!array_key_exists($key, $payload) || !is_string($payload[ $key ])) {
            throw new InvalidWorkflowDefinitionException("`{$key}` must be a string");
        }

        return $payload[ $key ];
    }

    /**
     * @param array<string, mixed> $payload
     * @return array<string, mixed>
     */
    private static function requiredArray(array $payload, string $key): array
    {
        if (!array_key_exists($key, $payload) || !is_array($payload[ $key ])) {
            throw new InvalidWorkflowDefinitionException("`{$key}` must be an object");
        }

        return $payload[ $key ];
    }

    /**
     * @param array<string, mixed> $payload
     * @return list<mixed>
     */
    private static function requiredList(array $payload, string $key): array
    {
        if (!array_key_exists($key, $payload) || !is_array($payload[ $key ])) {
            throw new InvalidWorkflowDefinitionException("`{$key}` must be an array");
        }

        return array_values($payload[ $key ]);
    }

    /**
     * @param array<string, mixed> $payload
     * @return array<string, mixed>|null
     */
    private static function optionalArray(array $payload, string $key): ?array
    {
        if (!array_key_exists($key, $payload) || $payload[ $key ] === null) {
            return null;
        }

        if (!is_array($payload[ $key ])) {
            throw new InvalidWorkflowDefinitionException("`{$key}` must be an object when present");
        }

        return $payload[ $key ];
    }

    /**
     * @param array<string, mixed> $payload
     * @return list<array<string, mixed>>
     */
    private static function optionalList(array $payload, string $key): array
    {
        if (!array_key_exists($key, $payload) || $payload[ $key ] === null) {
            return [];
        }

        if (!is_array($payload[ $key ])) {
            throw new InvalidWorkflowDefinitionException("`{$key}` must be an array when present");
        }

        return array_values($payload[ $key ]);
    }

    /**
     * @param array<string, mixed> $payload
     */
    private static function requiredValue(array $payload, string $key): mixed
    {
        if (!array_key_exists($key, $payload)) {
            throw new InvalidWorkflowDefinitionException("`{$key}` is required");
        }

        return $payload[ $key ];
    }

    /**
     * @param array<string, mixed> $payload
     */
    private static function requiredInteger(array $payload, string $key): int
    {
        if (!array_key_exists($key, $payload) || !is_int($payload[ $key ])) {
            throw new InvalidWorkflowDefinitionException("`{$key}` must be an integer");
        }

        return $payload[ $key ];
    }

    /**
     * @param array<string, mixed> $payload
     * @return list<string>
     */
    private static function optionalStringList(array $payload, string $key): array
    {
        if (!array_key_exists($key, $payload) || $payload[ $key ] === null) {
            return [];
        }

        if (!is_array($payload[ $key ])) {
            throw new InvalidWorkflowDefinitionException("`{$key}` must be an array when present");
        }

        $values = [];

        foreach ($payload[ $key ] as $value) {

            if (!is_string($value)) {
                throw new InvalidWorkflowDefinitionException("`{$key}` entries must be strings");
            }

            $values[] = $value;

        }

        return $values;
    }

    /**
     * @param array<string, mixed> $payload
     * @return list<array{name: string, bind: array<string, mixed>}>
     */
    private static function optionalToolBindings(array $payload): array
    {
        if (!array_key_exists('tools', $payload) || $payload[ 'tools' ] === null) {
            return [];
        }

        if (!is_array($payload[ 'tools' ])) {
            throw new InvalidWorkflowDefinitionException('`tools` must be an array when present');
        }

        $toolBindings = [];

        foreach ($payload[ 'tools' ] as $toolBinding) {

            if (!is_array($toolBinding)) {
                throw new InvalidWorkflowDefinitionException('tool bindings must be objects');
            }

            $toolName = self::requiredString($toolBinding, 'name');
            $toolBindValues = self::optionalArray($toolBinding, 'bind') ?? [];
            $toolBindings[] = [ 'name' => $toolName, 'bind' => $toolBindValues ];

        }

        return $toolBindings;
    }
}
