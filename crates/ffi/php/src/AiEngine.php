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

    private array $registeredToolInstances = [];

    private $toolInvocationHandler = null;

    private static ?self $activeEngineInstance = null;

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
        $this->customToolDefinitions = [];
        $this->registeredToolInstances = [];

        foreach ($customToolDefinitions as $customToolDefinition) {
            if ($customToolDefinition instanceof AiTool) {
                $this->registerToolInstance($customToolDefinition);

                continue;
            }

            if (!is_array($customToolDefinition)) {
                throw new InvalidArgumentException('Every tool passed to withTools() must be an AiTool or an array definition.');
            }

            $this->customToolDefinitions[] = $this->normalizeToolDefinition($customToolDefinition);
        }

        return $this;
    }

    public function withTool(AiTool $toolInstance): self
    {
        $this->registerToolInstance($toolInstance);

        return $this;
    }

    public function withToolDefinition(string $name, string $description, array $inputSchema, ?array $outputSchema = null): self
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
                self::clearRegisteredCallback();
            }
        }
    }

    public static function dispatchToolInvocation(string $toolInvocationRequestJson): string
    {
        if (!(self::$activeEngineInstance instanceof self)) {
            throw new RuntimeException('No active engine instance is available for tool dispatching.');
        }

        $toolInvocationRequest = self::decodeJson($toolInvocationRequestJson, 'tool invocation request');
        $toolName = self::requiredStringField($toolInvocationRequest, 'tool_name', 'tool invocation request');
        $toolInput = $toolInvocationRequest['tool_input'] ?? null;

        try {
            $toolOutput = self::$activeEngineInstance->invokeTool($toolName, $toolInput, $toolInvocationRequest);

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
        $allToolDefinitions = $this->customToolDefinitions;

        foreach ($this->registeredToolInstances as $registeredToolInstance) {
            $allToolDefinitions[] = $registeredToolInstance->definition();
        }

        return [
            'workflow_file_path' => $workflowFilePath,
            'workflow_input' => $this->workflowInput,
            'workflow_secrets' => $this->workflowSecrets,
            'custom_tools' => [
                'definitions' => $allToolDefinitions,
            ],
        ];
    }

    private function registerToolHandlerIfNeeded(): bool
    {
        if (!$this->needsToolCallback()) {
            return false;
        }

        if ($this->registeredToolInstances !== []) {
            self::$activeEngineInstance = $this;
            EngineAiFfi::registerToolCallback(self::toolDispatcherCallbackName());

            return true;
        }

        if ($this->toolInvocationHandler === null) {
            throw new RuntimeException('Custom tools were configured, but no tool handler was provided.');
        }

        self::$activeEngineInstance = $this;
        EngineAiFfi::registerToolCallback(self::toolDispatcherCallbackName());

        return true;
    }

    private function needsToolCallback(): bool
    {
        if ($this->registeredToolInstances !== []) {
            return true;
        }

        if ($this->toolInvocationHandler !== null) {
            return true;
        }

        return $this->customToolDefinitions !== [];
    }

    private function invokeTool(string $toolName, mixed $toolInput, array $toolInvocationRequest): mixed
    {
        if (array_key_exists($toolName, $this->registeredToolInstances)) {
            $registeredToolInstance = $this->registeredToolInstances[$toolName];
            $toolInputValues = self::normalizeToolInput($toolInput, $toolName);

            return $registeredToolInstance->invoke($toolInputValues, $this->workflowSecrets);
        }

        if (is_callable($this->toolInvocationHandler)) {
            return ($this->toolInvocationHandler)($toolName, $toolInput, $toolInvocationRequest);
        }

        throw new RuntimeException(sprintf('No registered tool implementation found for `%s`.', $toolName));
    }

    private static function normalizeToolInput(mixed $toolInput, string $toolName): array
    {
        if ($toolInput === null) {
            return [];
        }

        if (!is_array($toolInput)) {
            throw new RuntimeException(sprintf('Tool `%s` expected object input but received `%s`.', $toolName, gettype($toolInput)));
        }

        return $toolInput;
    }

    private function registerToolInstance(AiTool $toolInstance): void
    {
        $toolName = $toolInstance->name();

        if ($toolName === '') {
            throw new InvalidArgumentException('Tool name cannot be empty.');
        }

        if (array_key_exists($toolName, $this->registeredToolInstances)) {
            throw new InvalidArgumentException(sprintf('Duplicate tool registration for `%s`.', $toolName));
        }

        $this->registeredToolInstances[$toolName] = $toolInstance;
    }

    private static function resetActiveEngineInstance(): void
    {
        self::$activeEngineInstance = null;
    }

    private static function clearRegisteredCallback(): void
    {
        EngineAiFfi::clearToolCallback();
        self::resetActiveEngineInstance();
    }

    private static function toolDispatcherCallbackName(): string
    {
        return self::class . '::dispatchToolInvocation';
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
