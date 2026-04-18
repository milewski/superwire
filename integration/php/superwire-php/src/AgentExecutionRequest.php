<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class AgentExecutionRequest
{
    /**
     * @param array<string, mixed> $provider
     * @param array<string, mixed> $metadata
     * @param array<string, mixed> $localBindings
     */
    public function __construct(
        public readonly string $agentName,
        public readonly string $providerName,
        public readonly string $driverName,
        public readonly string $model,
        public readonly string $prompt,
        public readonly array $provider = [],
        public readonly mixed $context = null,
        public readonly mixed $inference = null,
        public readonly array $tools = [],
        public readonly array $metadata = [],
        public readonly array $localBindings = [],
    ) {
    }
}
