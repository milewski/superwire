<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use Spatie\LaravelData\Data;

final class WorkflowExecutionRequest extends Data
{
    /**
     * @param array<string, mixed> $inputPayload
     * @param array<string, mixed>|null $secretsPayload
     * @param array<int, array<string, mixed>> $customTools
     */
    public function __construct(
        public string $executionId,
        public string $workflowSource,
        public array $inputPayload,
        public ?array $secretsPayload,
        public array $customTools,
    )
    {
    }

    /**
     * Serializes the request payload expected by the FFI bridge.
     *
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $request = [
            'execution_id' => $this->executionId,
            'workflow_source' => $this->workflowSource,
            'input' => [
                'payload' => $this->inputPayload,
            ],
            'custom_tools' => $this->customTools,
        ];

        if ($this->secretsPayload !== null) {
            $request['secrets'] = [
                'payload' => $this->secretsPayload,
            ];
        }

        return $request;
    }
}
