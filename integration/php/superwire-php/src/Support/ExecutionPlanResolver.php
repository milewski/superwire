<?php

declare(strict_types=1);

namespace Superwire\Contracts\Support;

use Superwire\Contracts\AgentDefinition;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;

final class ExecutionPlanResolver
{
    /**
     * @param list<AgentDefinition> $agents
     * @return list<list<string>>
     */
    public function resolveBatches(array $agents): array
    {
        $agentNamesByName = [];
        $dependenciesByName = [];

        foreach ($agents as $agentDefinition) {
            $agentNamesByName[$agentDefinition->name] = true;
            $dependenciesByName[$agentDefinition->name] = $agentDefinition->dependencies;
        }

        foreach ($dependenciesByName as $agentName => $dependencyNames) {
            foreach ($dependencyNames as $dependencyName) {
                if (array_key_exists($dependencyName, $agentNamesByName)) {
                    continue;
                }

                throw new InvalidWorkflowDefinitionException(
                    "agent `{$agentName}` depends on unknown agent `{$dependencyName}`"
                );
            }
        }

        $resolvedAgentNames = [];
        $unresolvedAgentNames = array_fill_keys(array_keys($agentNamesByName), true);
        $executionBatches = [];

        while ($unresolvedAgentNames !== []) {
            $readyAgentNames = [];

            foreach (array_keys($unresolvedAgentNames) as $agentName) {
                $dependencyNames = $dependenciesByName[$agentName];
                $isBlocked = false;

                foreach ($dependencyNames as $dependencyName) {
                    if (array_key_exists($dependencyName, $resolvedAgentNames)) {
                        continue;
                    }

                    $isBlocked = true;

                    break;
                }

                if ($isBlocked) {
                    continue;
                }

                $readyAgentNames[] = $agentName;
            }

            if ($readyAgentNames === []) {
                throw new InvalidWorkflowDefinitionException('execution graph contains a cycle or unresolved dependency');
            }

            sort($readyAgentNames);
            $executionBatches[] = $readyAgentNames;

            foreach ($readyAgentNames as $readyAgentName) {
                unset($unresolvedAgentNames[$readyAgentName]);
                $resolvedAgentNames[$readyAgentName] = true;
            }
        }

        return $executionBatches;
    }
}
