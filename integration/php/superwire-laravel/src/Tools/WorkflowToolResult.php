<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use JsonSerializable;

final readonly class WorkflowToolResult implements JsonSerializable
{
    /**
     * @param array<string, mixed>|null $payload
     * @param array<string, mixed>|null $error
     */
    private function __construct(
        public string $status,
        public ?array $payload = null,
        public ?array $error = null,
    )
    {
    }

    /**
     * @param array<string, mixed> $payload
     */
    public static function success(array $payload): self
    {
        return new self(status: 'success', payload: $payload);
    }

    /**
     * @param array<string, mixed> $context
     */
    public static function fail(string $reason, array $context = []): self
    {
        return new self(
            status: 'error',
            error: [
                'reason' => $reason,
                'context' => $context,
            ],
        );
    }

    public function isSuccess(): bool
    {
        return $this->status === 'success';
    }

    public function reason(): ?string
    {
        return is_array($this->error) ? ($this->error['reason'] ?? null) : null;
    }

    public function jsonSerialize(): array
    {
        return array_filter([
            'status' => $this->status,
            'payload' => $this->payload,
            'error' => $this->error,
        ], static fn (mixed $value): bool => $value !== null);
    }
}
