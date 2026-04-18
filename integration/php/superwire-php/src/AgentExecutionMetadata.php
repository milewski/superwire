<?php

declare(strict_types=1);

namespace Superwire\Contracts;

final class AgentExecutionMetadata
{
    /**
     * @param list<string> $dependencies
     * @param list<string> $dependents
     */
    public function __construct(
        public readonly array $dependencies,
        public readonly array $dependents,
        public readonly int $batch,
        public readonly mixed $outputContract = null,
    ) {
    }
}
