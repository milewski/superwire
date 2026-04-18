<?php

declare(strict_types=1);

namespace Superwire\Laravel\Data;

use Spatie\LaravelData\Data;
use Superwire\Contracts\AgentExpectedOutput;
use Superwire\Contracts\AgentExecutionMetadata;
use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\ExecutionBindings;
use Superwire\Contracts\ProviderExecution;

final class ResolvedAgentExecutionData extends Data
{
    /**
     * @param list<ResolvedToolData> $tools
     * @param array<string, mixed> $localBindings
     * @param list<string> $dependencies
     * @param list<string> $dependents
     */
    public function __construct(
        public string $agentName,
        public ResolvedProviderData $provider,
        public string $model,
        public string $prompt,
        public mixed $context,
        public mixed $inference,
        public array $tools,
        public array $localBindings,
        public array $dependencies,
        public array $dependents,
        public int $batch,
        public mixed $expectedOutput,
    ) {
    }

    /**
     * @return array<string, mixed>
     */
    public function expectedOutputWorkflowType(): array
    {
        if (is_array($this->expectedOutput) && array_key_exists('workflow_type', $this->expectedOutput) && is_array($this->expectedOutput['workflow_type'])) {
            return $this->expectedOutput['workflow_type'];
        }

        return [];
    }

    public function toRequest(): AgentExecutionRequest
    {
        $resolvedTools = [];

        foreach ($this->tools as $tool) {
            $resolvedTools[] = $tool->toToolExecution();
        }

        return new AgentExecutionRequest(
            agentName: $this->agentName,
            provider: new ProviderExecution(
                name: $this->provider->name,
                provider: $this->provider->provider,
                config: $this->provider->config,
            ),
            model: $this->model,
            prompt: $this->prompt,
            expectedOutput: AgentExpectedOutput::fromContract(is_array($this->expectedOutput) ? $this->expectedOutput : []),
            context: $this->context,
            inference: $this->inference,
            tools: $resolvedTools,
            metadata: new AgentExecutionMetadata(
                dependencies: $this->dependencies,
                dependents: $this->dependents,
                batch: $this->batch,
                outputContract: $this->expectedOutput,
            ),
            bindings: new ExecutionBindings($this->localBindings),
        );
    }
}
