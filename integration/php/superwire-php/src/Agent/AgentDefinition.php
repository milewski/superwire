<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

final class AgentDefinition
{
    /**
     * @param list<array{name: string, bind: array<string, mixed>}> $tools
     * @param list<string> $dependencies
     * @param list<string> $dependents
     */
    public function __construct(
        public readonly string $name,
        public readonly string $provider,
        public readonly mixed $model,
        public readonly mixed $prompt,
        public readonly mixed $context,
        public readonly mixed $inference,
        public readonly array $tools,
        public readonly ?AgentForEachDefinition $forEach,
        public readonly mixed $output,
        public readonly array $dependencies,
        public readonly array $dependents,
        public readonly int $batch,
    ) {
    }
}
