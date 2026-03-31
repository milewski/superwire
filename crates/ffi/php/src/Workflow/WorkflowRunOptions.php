<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use Spatie\LaravelData\Data;

final class WorkflowRunOptions extends Data
{
    /**
     * @param ?string $requestId Optional bridge request id.
     * @param ?string $executionId Optional explicit workflow execution id.
     */
    public function __construct(
        public ?string $requestId = null,
        public ?string $executionId = null,
    )
    {
    }

    /**
     * Returns the request id passed to the engine bridge.
     */
    public function requestId(): ?string
    {
        return $this->requestId;
    }

    /**
     * Resolves the execution id using a fallback when needed.
     */
    public function resolveExecutionId(string $fallbackExecutionId): string
    {
        return $this->executionId ?? $fallbackExecutionId;
    }
}
