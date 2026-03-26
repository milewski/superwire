<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use InvalidArgumentException;
use RuntimeException;
use Throwable;

final class AiEngine
{
    private array $workflowInput = [];

    private array $workflowSecrets = [];

    private array $customToolDefinitions = [];

    private $toolInvocationHandler = null;

    private static $activeToolInvocationHandler = null;

    public function withInput(array $workflowInput): self
    {
        $this->workflowInput = $workflowInput;

        return $this;
    }

    public function withInputField(string $fieldName, mixed $fieldValue): self
    {
        $this->workflowInput[$fieldName] = $fieldValue;

        return $this;
    }

    public function withSecrets(array $workflowSecrets): self
    {
        $this->workflowSecrets = $workflowSecrets;

        return $this;
    }

    public function withSecret(string $secretName, mixed $secretValue): self
    {
        $this->workflowSecrets[$secretName] = $secretValue;

        return $this;
    }

    public function withTools(array $customToolDefinitions): self
    {
        $normalizedDefinitions = [];

        foreach ($customToolDefinitions as $customToolDefinition) {
            if (!is_array($customToolDefinition)) {
                throw new InvalidArgumentException('Every tool definition passed to withTools() must be an array.');
            }

            $normalizedDefinitions[] = $this->normalizeToolDefinition($customToolDefinition);
        }

        $this->customToolDefinitions = $normalizedDefinitions;

        return $this;
    }

    public function withTool(string $name, string $description, array $inputSchema, ?array $outputSchema = null): self
    {
        $this->customToolDefinitions[] = [
            'name' => $name,
            'description' => $description,
            'input_schema' => $inputSchema,
            'output_schema' => $outputSchema,
            'execution_contract' => 'host_callback',
        ];

        return $this;
    }

    public function withToolHandler(callable $toolInvocationHandler): self
    {
        $this->toolInvocationHandler = $toolInvocationHandler;

        return $this;
    }

    public function run(string $workflowFilePath): mixed
    {
        $workflowExecutionResponse = $this->runRaw($workflowFilePath);
        $workflowExecutionResult = self::requiredArrayField($workflowExecutionResponse, 'result', 'workflow response');
        $workflowExecutionStatus = self::requiredStringField($workflowExecutionResult, 'status', 'workflow result');

        if ($workflowExecutionStatus === 'succeeded') {
            return $workflowExecutionResult['workflow_output'] ?? null;
        }

        if ($workflowExecutionStatus === 'failed') {
            $workflowError = self::requiredArrayField($workflowExecutionResult, 'error', 'workflow failure result');
            $workflowErrorCode = self::requiredStringField($workflowError, 'code', 'workflow failure result');
            $workflowErrorMessage = self::requiredStringField($workflowError, 'message', 'workflow failure result');

            throw new RuntimeException(sprintf('[%s] %s', $workflowErrorCode, $workflowErrorMessage));
        }

        throw new RuntimeException(sprintf('Unknown workflow result status `%s`.', $workflowExecutionStatus));
    }

    public function runRaw(string $workflowFilePath): array
    {
        $toolCallbackRegistered = $this->registerToolHandlerIfNeeded();

        try {
            $workflowExecutionRequest = $this->buildWorkflowRequest($workflowFilePath);
            $workflowExecutionRequestJson = self::encodeJson($workflowExecutionRequest, 'workflow request');
            $workflowExecutionResponseJson = EngineAiFfi::executeWorkflow($workflowExecutionRequestJson);

            return self::decodeJson($workflowExecutionResponseJson, 'workflow response');
        } finally {
            if ($toolCallbackRegistered) {
                EngineAiFfi::clearToolCallback();
                self::$activeToolInvocationHandler = null;
            }
        }
    }

    public static function dispatchToolInvocation(string $toolInvocationRequestJson): string
    {
        if (!is_callable(self::$activeToolInvocationHandler)) {
            throw new RuntimeException('No tool handler is currently registered.');
        }

        $toolInvocationRequest = self::decodeJson($toolInvocationRequestJson, 'tool invocation request');
        $toolName = self::requiredStringField($toolInvocationRequest, 'tool_name', 'tool invocation request');
        $toolInput = $toolInvocationRequest['tool_input'] ?? null;

        try {
            $toolOutput = (self::$activeToolInvocationHandler)($toolName, $toolInput, $toolInvocationRequest);

            return self::encodeJson([
                'result' => [
                    'status' => 'succeeded',
                    'tool_output' => $toolOutput,
                ],
            ], 'tool invocation response');
        } catch (Throwable $throwable) {
            return self::encodeJson([
                'result' => [
                    'status' => 'failed',
                    'error' => [
                        'code' => 'tool_execution_failed',
                        'message' => $throwable->getMessage(),
                        'details' => [
                            'exception_class' => $throwable::class,
                        ],
                    ],
                ],
            ], 'tool invocation response');
        }
    }

    private function buildWorkflowRequest(string $workflowFilePath): array
    {
        return [
            'workflow_file_path' => $workflowFilePath,
            'workflow_input' => $this->workflowInput,
            'workflow_secrets' => $this->workflowSecrets,
            'custom_tools' => [
                'definitions' => $this->customToolDefinitions,
            ],
        ];
    }

    private function registerToolHandlerIfNeeded(): bool
    {
        if ($this->toolInvocationHandler === null) {
            if ($this->customToolDefinitions !== []) {
                throw new RuntimeException('Custom tools were configured, but no tool handler was provided.');
            }

            return false;
        }

        self::$activeToolInvocationHandler = $this->toolInvocationHandler;
        EngineAiFfi::registerToolCallback(self::toolDispatcherCallbackName());

        return true;
    }

    private function normalizeToolDefinition(array $customToolDefinition): array
    {
        $toolName = self::requiredStringField($customToolDefinition, 'name', 'tool definition');
        $toolDescription = self::requiredStringField($customToolDefinition, 'description', 'tool definition');

        $inputSchema = $customToolDefinition['input_schema'] ?? $customToolDefinition['inputSchema'] ?? null;

        if (!is_array($inputSchema)) {
            throw new InvalidArgumentException('Tool definition `input_schema` must be an array.');
        }

        $outputSchema = $customToolDefinition['output_schema'] ?? $customToolDefinition['outputSchema'] ?? null;

        if ($outputSchema !== null && !is_array($outputSchema)) {
            throw new InvalidArgumentException('Tool definition `output_schema` must be an array when provided.');
        }

        $executionContract = $customToolDefinition['execution_contract'] ?? $customToolDefinition['executionContract'] ?? 'host_callback';

        if (!is_string($executionContract)) {
            throw new InvalidArgumentException('Tool definition `execution_contract` must be a string.');
        }

        if ($executionContract !== 'host_callback') {
            throw new InvalidArgumentException('Tool definition `execution_contract` must be `host_callback`.');
        }

        return [
            'name' => $toolName,
            'description' => $toolDescription,
            'input_schema' => $inputSchema,
            'output_schema' => $outputSchema,
            'execution_contract' => $executionContract,
        ];
    }

    private static function toolDispatcherCallbackName(): string
    {
        return self::class . '::dispatchToolInvocation';
    }

    private static function decodeJson(string $jsonPayload, string $context): array
    {
        try {
            $decodedJson = json_decode($jsonPayload, true, 512, JSON_THROW_ON_ERROR);
        } catch (Throwable $throwable) {
            throw new RuntimeException(sprintf('Failed to decode %s JSON: %s', $context, $throwable->getMessage()), 0, $throwable);
        }

        if (!is_array($decodedJson)) {
            throw new RuntimeException(sprintf('Expected %s JSON object payload.', $context));
        }

        return $decodedJson;
    }

    private static function encodeJson(mixed $value, string $context): string
    {
        try {
            return json_encode($value, JSON_THROW_ON_ERROR);
        } catch (Throwable $throwable) {
            throw new RuntimeException(sprintf('Failed to encode %s JSON: %s', $context, $throwable->getMessage()), 0, $throwable);
        }
    }

    private static function requiredStringField(array $payload, string $fieldName, string $context): string
    {
        $fieldValue = $payload[$fieldName] ?? null;

        if (!is_string($fieldValue) || $fieldValue === '') {
            throw new InvalidArgumentException(sprintf('Expected non-empty string field `%s` in %s.', $fieldName, $context));
        }

        return $fieldValue;
    }

    private static function requiredArrayField(array $payload, string $fieldName, string $context): array
    {
        $fieldValue = $payload[$fieldName] ?? null;

        if (!is_array($fieldValue)) {
            throw new InvalidArgumentException(sprintf('Expected object field `%s` in %s.', $fieldName, $context));
        }

        return $fieldValue;
    }
}
