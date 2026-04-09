<?php

namespace Superwire\Laravel\Exceptions;

class WorkflowExecutionException extends SuperwireException
{
    /**
     * @param array<int, string>|null $command
     * @param array<string, mixed>|null $errorPayload
     */
    public function __construct(
        string $message,
        private readonly ?array $command = null,
        private readonly ?array $errorPayload = null,
        private readonly ?string $rawCliOutput = null,
    ) {
        parent::__construct($message);
    }

    /**
     * @return array<int, string>|null
     */
    public function command(): ?array
    {
        return $this->command;
    }

    /**
     * @return array<string, mixed>|null
     */
    public function errorPayload(): ?array
    {
        return $this->errorPayload;
    }

    /**
     * @return array<string, mixed>
     */
    public function context(): array
    {
        $context = $this->errorPayload['details']['context'] ?? null;

        if (is_array($context)) {
            return $context;
        }

        return [];
    }

    public function rawCliOutput(): ?string
    {
        return $this->rawCliOutput;
    }
}
