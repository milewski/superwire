<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

class EngineExecutionResult
{
    public readonly string $executionId;

    private EngineFfiBridge $engineFfiBridge;

    private bool $hasCachedSuccessValue;

    private mixed $cachedSuccessValue;

    private bool $hasCachedErrorValue;

    private ?array $cachedErrorValue;

    private bool $hasCachedContextValue;

    private mixed $cachedContextValue;

    private bool $canReadDeferredValues;

    public function __construct(
        EngineFfiBridge $engineFfiBridge,
        string $executionId,
        ?array $eagerError = null,
        bool $canReadDeferredValues = true,
    ) {
        $this->engineFfiBridge = $engineFfiBridge;
        $this->executionId = $executionId;
        $this->hasCachedSuccessValue = $eagerError !== null;
        $this->cachedSuccessValue = null;
        $this->hasCachedErrorValue = $eagerError !== null;
        $this->cachedErrorValue = $eagerError;
        $this->hasCachedContextValue = false;
        $this->cachedContextValue = null;
        $this->canReadDeferredValues = $canReadDeferredValues;
    }

    public function isSuccess(): bool
    {
        return $this->success() !== null;
    }

    public function isError(): bool
    {
        return $this->error() !== null;
    }

    public function success(): mixed
    {
        if ($this->hasCachedSuccessValue) {
            return $this->cachedSuccessValue;
        }

        if (!$this->canReadDeferredValues) {
            $this->hasCachedSuccessValue = true;
            $this->cachedSuccessValue = null;

            return $this->cachedSuccessValue;
        }

        try {
            $responseValue = $this->readExecutionValue(ExecutionValueName::SUCCESS);
        } catch (\Throwable $throwable) {
            if (!$this->hasCachedErrorValue) {
                $this->hasCachedErrorValue = true;
                $this->cachedErrorValue = [
                    'code' => 'execution_failed',
                    'message' => $throwable->getMessage(),
                ];
            }

            $this->hasCachedSuccessValue = true;
            $this->cachedSuccessValue = null;

            return $this->cachedSuccessValue;
        }

        $this->hasCachedSuccessValue = true;
        $this->cachedSuccessValue = $responseValue;

        return $this->cachedSuccessValue;
    }

    public function error(): ?array
    {
        if ($this->hasCachedErrorValue) {
            return $this->cachedErrorValue;
        }

        if (!$this->canReadDeferredValues) {
            $this->hasCachedErrorValue = true;
            $this->cachedErrorValue = null;

            return $this->cachedErrorValue;
        }

        try {
            $responseValue = $this->readExecutionValue(ExecutionValueName::ERROR);
        } catch (\Throwable $throwable) {
            $this->hasCachedErrorValue = true;
            $this->cachedErrorValue = [
                'code' => 'execution_failed',
                'message' => $throwable->getMessage(),
            ];

            return $this->cachedErrorValue;
        }

        if (!\is_array($responseValue)) {
            $this->hasCachedErrorValue = true;
            $this->cachedErrorValue = null;

            return $this->cachedErrorValue;
        }

        $this->hasCachedErrorValue = true;
        $this->cachedErrorValue = [
            'code' => \is_string($responseValue['code'] ?? null) ? $responseValue['code'] : 'execution_failed',
            'message' => \is_string($responseValue['message'] ?? null)
                ? $responseValue['message']
                : 'Unknown workflow execution error',
            'context' => $responseValue['context'] ?? null,
            'details' => $responseValue['details'] ?? null,
        ];

        return $this->cachedErrorValue;
    }

    public function context(): mixed
    {
        if ($this->hasCachedContextValue) {
            return $this->cachedContextValue;
        }

        if (!$this->canReadDeferredValues) {
            $this->hasCachedContextValue = true;
            $this->cachedContextValue = null;

            return $this->cachedContextValue;
        }

        try {
            $this->cachedContextValue = $this->readExecutionValue(ExecutionValueName::CONTEXT);
        } catch (\Throwable) {
            $this->cachedContextValue = null;
        }

        $this->hasCachedContextValue = true;

        return $this->cachedContextValue;
    }

    private function readExecutionValue(string $valueName): mixed
    {
        $readExecutionValueEnvelope = $this->engineFfiBridge->readExecutionValue([
            'execution_id' => $this->executionId,
            'value' => $valueName,
        ]);

        if (($readExecutionValueEnvelope['status'] ?? null) === 'failed') {
            $error = \is_array($readExecutionValueEnvelope['error'] ?? null) ? $readExecutionValueEnvelope['error'] : [];
            $errorCode = \is_string($error['code'] ?? null) ? $error['code'] : 'execution_failed';
            $errorMessage = \is_string($error['message'] ?? null) ? $error['message'] : 'Unknown deferred execution error.';

            throw new \RuntimeException("[{$errorCode}] {$errorMessage}");
        }

        if (!\is_array($readExecutionValueEnvelope['result'] ?? null)) {
            throw new \RuntimeException('Deferred execution result envelope is missing `result`.');
        }

        return $readExecutionValueEnvelope['result']['value'] ?? null;
    }
}
