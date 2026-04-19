<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use JsonSerializable;

final class WorkflowToolResult implements JsonSerializable
{
    private function __construct(
        private readonly WorkflowToolResultStatus $status,
        private readonly mixed $payload,
        private readonly ?string $errorReason,
        private readonly mixed $errorDetails,
    ) {
    }

    public static function success(mixed $payload = null): self
    {
        return new self(
            status: WorkflowToolResultStatus::Success,
            payload: $payload,
            errorReason: null,
            errorDetails: null,
        );
    }

    public static function fail(string $reason, mixed $details = null): self
    {
        return new self(
            status: WorkflowToolResultStatus::Error,
            payload: null,
            errorReason: $reason,
            errorDetails: $details,
        );
    }

    public function jsonSerialize(): array
    {
        if ($this->status === WorkflowToolResultStatus::Success) {

            return [
                'status' => $this->status->value,
                'payload' => $this->payload,
            ];

        }

        $error = [
            'reason' => $this->errorReason,
        ];

        if ($this->errorDetails !== null) {
            $error[ 'details' ] = $this->errorDetails;
        }

        return [
            'status' => $this->status->value,
            'error' => $error,
        ];
    }
}
