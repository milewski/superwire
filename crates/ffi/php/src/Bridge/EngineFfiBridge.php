<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use JsonException;
use RuntimeException;

class EngineFfiBridge
{
    public function __construct(array $options = [])
    {
        if (\array_key_exists('libraryPath', $options) && $options['libraryPath'] !== null) {
            // The PHP extension links directly to Rust internals and does not load an external shared object path.
        }
    }

    public function executeWorkflow(array $workflowExecutionRequest, array $options = []): array
    {
        $responseEnvelope = $this->invoke([
            'protocol_version' => FFI_PROTOCOL_VERSION,
            'request_id' => $options['requestId'] ?? null,
            'operation' => FfiOperation::EXECUTE_WORKFLOW,
            'payload' => $workflowExecutionRequest,
        ]);

        if (($responseEnvelope['operation'] ?? null) !== FfiOperation::EXECUTE_WORKFLOW) {
            throw new RuntimeException('Unexpected FFI response operation for executeWorkflow.');
        }

        if (!\array_key_exists('payload', $responseEnvelope) || !\is_array($responseEnvelope['payload'])) {
            throw new RuntimeException('FFI executeWorkflow response payload is missing.');
        }

        return $responseEnvelope['payload'];
    }

    public function invokeTool(array $toolInvocationPayload, array $options = []): array
    {
        $responseEnvelope = $this->invoke([
            'protocol_version' => FFI_PROTOCOL_VERSION,
            'request_id' => $options['requestId'] ?? null,
            'operation' => FfiOperation::INVOKE_TOOL,
            'payload' => $toolInvocationPayload,
        ]);

        if (($responseEnvelope['operation'] ?? null) !== FfiOperation::INVOKE_TOOL) {
            throw new RuntimeException('Unexpected FFI response operation for invokeTool.');
        }

        if (!\array_key_exists('payload', $responseEnvelope) || !\is_array($responseEnvelope['payload'])) {
            throw new RuntimeException('FFI invokeTool response payload is missing.');
        }

        return $responseEnvelope['payload'];
    }

    public function readExecutionValue(array $readExecutionValueRequest, array $options = []): array
    {
        $responseEnvelope = $this->invoke([
            'protocol_version' => FFI_PROTOCOL_VERSION,
            'request_id' => $options['requestId'] ?? null,
            'operation' => FfiOperation::READ_EXECUTION_VALUE,
            'payload' => $readExecutionValueRequest,
        ]);

        if (($responseEnvelope['operation'] ?? null) !== FfiOperation::READ_EXECUTION_VALUE) {
            throw new RuntimeException('Unexpected FFI response operation for readExecutionValue.');
        }

        if (!\array_key_exists('payload', $responseEnvelope) || !\is_array($responseEnvelope['payload'])) {
            throw new RuntimeException('FFI readExecutionValue response payload is missing.');
        }

        return $responseEnvelope['payload'];
    }

    public function invoke(array $requestEnvelope): array
    {
        $boundaryEnvelope = $this->invokeBoundary($requestEnvelope);
        $boundaryStatus = $boundaryEnvelope['status'] ?? null;

        if ($boundaryStatus === 'failed') {
            $boundaryError = \is_array($boundaryEnvelope['error'] ?? null) ? $boundaryEnvelope['error'] : [];
            $boundaryErrorCode = \is_string($boundaryError['code'] ?? null) ? $boundaryError['code'] : 'unknown';
            $boundaryErrorMessage = \is_string($boundaryError['message'] ?? null)
                ? $boundaryError['message']
                : 'Unknown FFI boundary error';

            throw new RuntimeException("FFI boundary error ({$boundaryErrorCode}): {$boundaryErrorMessage}");
        }

        if ($boundaryStatus !== 'succeeded') {
            throw new RuntimeException('Unknown FFI boundary status.');
        }

        if (!\array_key_exists('response', $boundaryEnvelope) || !\is_array($boundaryEnvelope['response'])) {
            throw new RuntimeException('FFI response envelope is missing.');
        }

        $responseEnvelope = $boundaryEnvelope['response'];

        if (($responseEnvelope['protocol_version'] ?? null) !== FFI_PROTOCOL_VERSION) {
            throw new RuntimeException('Unsupported FFI protocol version in response envelope.');
        }

        return $responseEnvelope;
    }

    public function close(): void
    {
        // no-op for PHP bridge compatibility with the JavaScript API
    }

    private function invokeBoundary(array $requestEnvelope): array
    {
        try {
            $requestPayload = \json_encode($requestEnvelope, JSON_THROW_ON_ERROR);
        } catch (JsonException $jsonException) {
            throw new RuntimeException('Failed to serialize FFI request envelope.', previous: $jsonException);
        }

        $responsePayload = NativeExtension::invokeJson($requestPayload);

        try {
            $boundaryEnvelope = \json_decode($responsePayload, true, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException $jsonException) {
            throw new RuntimeException('Failed to parse FFI response payload.', previous: $jsonException);
        }

        if (!\is_array($boundaryEnvelope)) {
            throw new RuntimeException('FFI boundary envelope must be a JSON object.');
        }

        return $boundaryEnvelope;
    }
}
