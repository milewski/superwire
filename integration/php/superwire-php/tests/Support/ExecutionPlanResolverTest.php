<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Agent\AgentDefinition;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\ExecutionPlanResolver;

final class ExecutionPlanResolverTest extends TestCase
{
    public function test_it_groups_independent_agents_in_parallel_batches(): void
    {
        $resolver = new ExecutionPlanResolver();

        $batches = $resolver->resolveBatches([
            $this->agentDefinition('changelog', []),
            $this->agentDefinition('social_thread', []),
            $this->agentDefinition('customer_email', []),
            $this->agentDefinition('review', [ 'changelog', 'social_thread', 'customer_email' ]),
        ]);

        $this->assertSame(
            expected: [ [ 'changelog', 'customer_email', 'social_thread' ], [ 'review' ] ],
            actual: $batches,
        );
    }

    public function test_it_rejects_unknown_dependencies(): void
    {
        $resolver = new ExecutionPlanResolver();

        $this->expectException(InvalidWorkflowDefinitionException::class);

        $resolver->resolveBatches([
            $this->agentDefinition('summary', [ 'missing' ]),
        ]);
    }

    public function test_it_rejects_cyclic_dependencies(): void
    {
        $resolver = new ExecutionPlanResolver();

        $this->expectException(InvalidWorkflowDefinitionException::class);
        $this->expectExceptionMessage('execution graph contains a cycle or unresolved dependency');

        $resolver->resolveBatches([
            $this->agentDefinition('alpha', [ 'beta' ]),
            $this->agentDefinition('beta', [ 'alpha' ]),
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
            output: [ 'final_output' => [ 'workflow_type' => [ 'kind' => 'string' ] ] ],
            dependencies: $dependencies,
            dependents: [],
            batch: 0,
        );
    }
}
