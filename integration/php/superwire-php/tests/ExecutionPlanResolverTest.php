<?php

declare(strict_types=1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\AgentDefinition;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\ExecutionPlanResolver;

final class ExecutionPlanResolverTest extends TestCase
{
    public function testItGroupsIndependentAgentsInParallelBatches(): void
    {
        $resolver = new ExecutionPlanResolver();

        $batches = $resolver->resolveBatches([
            $this->agentDefinition('changelog', []),
            $this->agentDefinition('social_thread', []),
            $this->agentDefinition('customer_email', []),
            $this->agentDefinition('review', ['changelog', 'social_thread', 'customer_email']),
        ]);

        self::assertSame(
            [
                ['changelog', 'customer_email', 'social_thread'],
                ['review'],
            ],
            $batches
        );
    }

    public function testItRejectsUnknownDependencies(): void
    {
        $resolver = new ExecutionPlanResolver();

        $this->expectException(InvalidWorkflowDefinitionException::class);

        $resolver->resolveBatches([
            $this->agentDefinition('summary', ['missing']),
        ]);
    }

    private function agentDefinition(string $name, array $dependencies): AgentDefinition
    {
        return new AgentDefinition(
            name: $name,
            provider: 'openai',
            model: 'gpt-4.1-mini',
            prompt: 'prompt',
            context: null,
            inference: null,
            tools: [],
            forEach: null,
            output: ['final_output' => ['workflow_type' => ['kind' => 'string']]],
            dependencies: $dependencies,
            dependents: [],
            batch: 0,
        );
    }
}
