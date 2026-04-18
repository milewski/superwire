<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Support\Stages;

use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Workflow\WorkflowDefinition;

final class WorkflowDefinitionValidationStage
{
    public function validate(WorkflowDefinition $workflowDefinition): void
    {
        $providerNames = [];

        foreach ($workflowDefinition->providers as $providerDefinition) {
            $providerNames[ $providerDefinition->name ] = true;
        }

        $agentNames = [];

        foreach ($workflowDefinition->agents as $agentDefinition) {
            $agentNames[ $agentDefinition->name ] = true;
        }

        foreach ($workflowDefinition->agents as $agentDefinition) {

            if (!array_key_exists($agentDefinition->provider, $providerNames)) {

                throw new InvalidWorkflowDefinitionException(
                    "agent `{$agentDefinition->name}` references unknown provider `{$agentDefinition->provider}`",
                );

            }

            foreach ($agentDefinition->dependencies as $dependencyName) {

                if (array_key_exists($dependencyName, $agentNames)) {
                    continue;
                }

                throw new InvalidWorkflowDefinitionException(
                    "agent `{$agentDefinition->name}` depends on unknown agent `{$dependencyName}`",
                );

            }

        }
    }
}
