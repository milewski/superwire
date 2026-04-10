<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

use Spatie\LaravelData\Data;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;

final readonly class ToolExecutionSignature
{
    /**
     * @param class-string<ToolInputData> $agentInputClass
     * @param class-string<ToolBoundInputData> $boundInputClass
     * @param class-string<Data> $outputClass
     * @param list<ToolHandleParameter> $handleParameters
     */
    public function __construct(
        public string $agentInputClass,
        public string $boundInputClass,
        public string $outputClass,
        private array $handleParameters,
    )
    {
    }

    /**
     * @return list<ToolHandleParameter>
     */
    public function handleParameters(): array
    {
        return $this->handleParameters;
    }
}
