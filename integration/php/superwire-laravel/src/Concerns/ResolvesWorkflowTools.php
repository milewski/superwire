<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Concerns;

use RuntimeException;
use Superwire\Laravel\AgentExecutionResult;
use Superwire\Laravel\Data\Agent\Agent;
use Superwire\Laravel\Tools\WorkflowTool;

trait ResolvesWorkflowTools
{
    /**
     * @return array<int, string|WorkflowTool>
     */
    private function resolveToolsForAgent(Agent $agent): array
    {
        if ($agent->tools === []) {
            return [];
        }

        $availableToolsByName = [];

        foreach ($this->tools as $tool) {

            $toolName = $this->resolveConfiguredToolName($tool);
            $availableToolsByName[ $toolName ] = $tool;

        }

        $resolvedTools = [];

        foreach ($agent->tools as $toolDefinition) {

            $toolName = is_array($toolDefinition) ? ($toolDefinition[ 'name' ] ?? null) : null;

            if (!is_string($toolName) || !array_key_exists($toolName, $availableToolsByName)) {
                throw new RuntimeException(sprintf('Tool %s is not configured for agent %s.', (string) $toolName, $agent->name));
            }

            $resolvedTools[] = $availableToolsByName[ $toolName ];

        }

        return $resolvedTools;
    }

    /**
     * @param array<string, AgentExecutionResult> $agentOutputs
     * @param array<string, mixed> $scope
     * @return array<string, array<string, mixed>>
     */
    private function resolveToolBindingsForAgent(Agent $agent, array $agentOutputs, array $scope): array
    {
        $resolvedBindings = [];

        foreach ($agent->tools as $toolDefinition) {

            if (!is_array($toolDefinition)) {
                continue;
            }

            $toolName = $toolDefinition[ 'name' ] ?? null;

            if (!is_string($toolName)) {
                continue;
            }

            $toolBind = $toolDefinition[ 'bind' ] ?? [];

            if (!is_array($toolBind)) {
                $toolBind = [];
            }

            $resolvedBindings[ $toolName ] = $this->resolveToolBindingValues($toolBind, $agentOutputs, $scope);

        }

        return $resolvedBindings;
    }

    /**
     * @param array<string, AgentExecutionResult> $agentOutputs
     * @param array<string, mixed> $scope
     */
    private function resolveToolBindingValues(array $toolBind, array $agentOutputs, array $scope): array
    {
        $resolvedValues = [];

        foreach ($toolBind as $key => $value) {
            $resolvedValues[ $key ] = $this->resolveToolBindingValue($value, $agentOutputs, $scope);
        }

        return $resolvedValues;
    }

    /**
     * @param array<string, AgentExecutionResult> $agentOutputs
     * @param array<string, mixed> $scope
     */
    private function resolveToolBindingValue(mixed $value, array $agentOutputs, array $scope): mixed
    {
        if (!is_array($value)) {
            return $value;
        }

        if (array_keys($value) === [ '$ref' ] && is_string($value[ '$ref' ])) {
            return $this->promptParser->resolveReference($value[ '$ref' ], $agentOutputs, $scope, $this->inputValues, $this->secretValues);
        }

        $resolvedValue = [];

        foreach ($value as $key => $nestedValue) {
            $resolvedValue[ $key ] = $this->resolveToolBindingValue($nestedValue, $agentOutputs, $scope);
        }

        return $resolvedValue;
    }

    private function resolveConfiguredToolName(mixed $tool): string
    {
        if (is_string($tool)) {

            if (is_a($tool, WorkflowTool::class, true)) {
                return $tool::name();
            }

            throw new RuntimeException(sprintf('Unsupported tool configuration type: %s', $tool));

        }

        if ($tool instanceof WorkflowTool) {
            return $tool::name();
        }

        throw new RuntimeException(sprintf('Unsupported tool configuration type: %s', get_debug_type($tool)));
    }
}
