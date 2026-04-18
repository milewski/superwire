<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class AgentExecutionRequest
{
    /**
     * @param list<ToolExecution> $tools
     */
    public function __construct(
        public readonly string $agentName,
        public readonly ProviderExecution $provider,
        public readonly string $model,
        public readonly string $prompt,
        public readonly AgentExpectedOutput $expectedOutput,
        public readonly mixed $context = null,
        public readonly mixed $inference = null,
        public readonly array $tools = [],
        public readonly ?AgentExecutionMetadata $metadata = null,
        public readonly ?ExecutionBindings $bindings = null,
    ) {
    }
}
