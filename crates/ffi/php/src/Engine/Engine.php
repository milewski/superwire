<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use InvalidArgumentException;
use RuntimeException;
use Throwable;

class Engine
{
    private EngineFfiBridge $engineFfiBridge;

    /**
     * @var callable(): string
     */
    private $executionIdGenerator;

    /**
     * @var array<string, array{tool: Tool, bounded: array}>
     */
    private array $registeredToolsByName;

    public function __construct(array $options = [])
    {
        $providedBridge = $options[ 'bridge' ] ?? null;

        if ($providedBridge !== null && !$providedBridge instanceof EngineFfiBridge) {
            throw new InvalidArgumentException('Engine `bridge` must be an EngineFfiBridge instance when provided.');
        }

        $this->engineFfiBridge = $providedBridge ?? new EngineFfiBridge($options[ 'bridgeOptions' ] ?? []);

        $providedExecutionIdGenerator = $options[ 'executionIdGenerator' ] ?? null;

        if ($providedExecutionIdGenerator !== null && !\is_callable($providedExecutionIdGenerator)) {
            throw new InvalidArgumentException('Engine `executionIdGenerator` must be callable when provided.');
        }

        $this->executionIdGenerator = $providedExecutionIdGenerator ?? fn (): string => $this->generateExecutionId();
        $this->registeredToolsByName = [];
    }

    public function registerGlobalTool(Tool $tool, array $options = []): self
    {
        $boundedArguments = \is_array($options[ 'bounded' ] ?? null) ? $options[ 'bounded' ] : [];

        $this->registeredToolsByName[ $tool->name ] = [
            'tool' => $tool,
            'bounded' => $boundedArguments,
        ];

        return $this;
    }

    public function registerTool(Tool $tool, array $options = []): self
    {
        return $this->registerGlobalTool($tool, $options);
    }

    public function unregisterTool(string $toolName): bool
    {
        if (!\array_key_exists($toolName, $this->registeredToolsByName)) {
            return false;
        }

        unset($this->registeredToolsByName[ $toolName ]);

        return true;
    }

    public function unregisterGlobalTool(string $toolName): bool
    {
        return $this->unregisterTool($toolName);
    }

    /**
     * @return array<int, Tool>
     */
    public function registeredTools(): array
    {
        $registeredTools = [];

        foreach ($this->registeredToolsByName as $registeredToolEntry) {
            $registeredTools[] = $registeredToolEntry[ 'tool' ];
        }

        return $registeredTools;
    }

    /**
     * @return array<int, Tool>
     */
    public function registeredGlobalTools(): array
    {
        return $this->registeredTools();
    }

    public function invokeTool(string $toolName, array $input): mixed
    {
        $registeredTool = $this->registeredToolsByName[ $toolName ] ?? null;

        if (!\is_array($registeredTool) || !$registeredTool[ 'tool' ] instanceof Tool) {
            throw new RuntimeException("Tool `{$toolName}` is not registered. Call engine->registerGlobalTool(...) first.");
        }

        return $registeredTool[ 'tool' ]->invoke(new ToolData(
            input: $input,
            bounded: $registeredTool[ 'bounded' ],
            context: [],
            inputType: $registeredTool[ 'tool' ]->inputType(),
            boundedType: $registeredTool[ 'tool' ]->boundedType(),
        ));
    }

    public function run(Workflow $workflow): EngineExecutionResult
    {
        $executionId = '';

        try {

            $defaultExecutionId = ($this->executionIdGenerator)();

            if (!\is_string($defaultExecutionId) || $defaultExecutionId === '') {
                throw new RuntimeException('Execution ID generator must return a non-empty string.');
            }

            $executionId = $workflow->executionId($defaultExecutionId);
            $workflowExecutionRequest = $workflow->toExecutionRequest($executionId);
            $workflowExecutionRequest[ 'custom_tools' ] = $this->resolveCustomToolDeclarations($workflowExecutionRequest[ 'custom_tools' ]);
            $workflowExecutionRequest[ 'defer_output' ] = true;

            $runtimeToolsByName = $this->resolveRuntimeToolsByName($workflow);
            $runtimeToolCallbackServer = null;

            if ($runtimeToolsByName !== []) {

                $runtimeToolCallbackServer = $this->startRuntimeToolCallbackServer($runtimeToolsByName);

                $workflowExecutionRequest[ 'tool_callback' ] = [
                    'endpoint' => $runtimeToolCallbackServer[ 'endpoint' ],
                    'auth_token' => $runtimeToolCallbackServer[ 'authToken' ],
                ];

            }

            try {

                $workflowExecutionEnvelope = $this->engineFfiBridge->executeWorkflow($workflowExecutionRequest, [
                    'requestId' => $workflow->requestId(),
                ]);

            } finally {
                if ($runtimeToolCallbackServer !== null) {
                    $this->stopRuntimeToolCallbackServer($runtimeToolCallbackServer);
                }
            }

            if (($workflowExecutionEnvelope[ 'status' ] ?? null) === 'failed') {

                $workflowError = \is_array($workflowExecutionEnvelope[ 'error' ] ?? null) ? $workflowExecutionEnvelope[ 'error' ] : [];

                return new EngineExecutionResult(
                    $this->engineFfiBridge,
                    $executionId,
                    [
                        'code' => \is_string($workflowError[ 'code' ] ?? null) ? $workflowError[ 'code' ] : 'execution_failed',
                        'message' => \is_string($workflowError[ 'message' ] ?? null)
                            ? $workflowError[ 'message' ]
                            : 'Unknown workflow execution error.',
                        'context' => $workflowError[ 'context' ] ?? null,
                        'details' => $workflowError[ 'details' ] ?? null,
                    ],
                    true,
                );

            }

            $outputEnvelope = \is_array($workflowExecutionEnvelope[ 'output' ] ?? null)
                ? $workflowExecutionEnvelope[ 'output' ]
                : [];
            $resultExecutionId = \is_string($outputEnvelope[ 'execution_id' ] ?? null) ? $outputEnvelope[ 'execution_id' ] : $executionId;

            return new EngineExecutionResult($this->engineFfiBridge, $resultExecutionId);

        } catch (Throwable $throwable) {

            $fallbackExecutionId = $executionId !== '' ? $executionId : ($this->executionIdGenerator)();

            return new EngineExecutionResult(
                $this->engineFfiBridge,
                $fallbackExecutionId,
                [
                    'code' => 'execution_failed',
                    'message' => $throwable->getMessage(),
                ],
                false,
            );

        }
    }

    public function close(): void
    {
        $this->engineFfiBridge->close();
    }

    private function generateExecutionId(): string
    {
        $timestamp = (string) \round(\microtime(true) * 1000);
        $randomSuffix = \bin2hex(\random_bytes(4));

        return "execution-{$timestamp}-{$randomSuffix}";
    }

    /**
     * @param array<int, array> $workflowDeclaredTools
     *
     * @return array<int, array>
     */
    private function resolveCustomToolDeclarations(array $workflowDeclaredTools): array
    {
        $customToolDeclarationsByName = [];

        foreach ($this->registeredTools() as $registeredTool) {
            $customToolDeclarationsByName[ $registeredTool->name ] = $registeredTool->toDeclaration();
        }

        foreach ($workflowDeclaredTools as $customToolDeclaration) {

            if (!\is_array($customToolDeclaration) || !\is_string($customToolDeclaration[ 'name' ] ?? null)) {
                continue;
            }

            $customToolDeclarationsByName[ $customToolDeclaration[ 'name' ]] = $customToolDeclaration;

        }

        return \array_values($customToolDeclarationsByName);
    }

    private function hasRuntimeTools(Workflow $workflow): bool
    {
        return $this->registeredToolsByName !== [] || $workflow->scopedTools() !== [];
    }

    /**
     * @return array<string, array{tool: Tool, bounded: array<string, mixed>}>
     */
    private function resolveRuntimeToolsByName(Workflow $workflow): array
    {
        $runtimeToolsByName = $this->registeredToolsByName;

        foreach ($workflow->scopedToolsByName() as $toolName => $tool) {

            $runtimeToolsByName[ $toolName ] = [
                'tool' => $tool,
                'bounded' => [],
            ];

        }

        return $runtimeToolsByName;
    }

    /**
     * @param array<string, array{tool: Tool, bounded: array<string, mixed>}> $runtimeToolsByName
     *
     * @return array{pid: int, endpoint: string, authToken: string}
     */
    private function startRuntimeToolCallbackServer(array $runtimeToolsByName): array
    {
        if (!\function_exists('pcntl_fork')) {
            throw new RuntimeException('The `pcntl` extension is required for in-process runtime tool callbacks.');
        }

        $socketServer = @\stream_socket_server('tcp://127.0.0.1:0', $errorNumber, $errorMessage);

        if (!\is_resource($socketServer)) {
            throw new RuntimeException("Unable to start runtime tool callback server: {$errorMessage} ({$errorNumber}).");
        }

        $socketAddress = \stream_socket_get_name($socketServer, false);

        if (!\is_string($socketAddress) || !\str_contains($socketAddress, ':')) {

            \fclose($socketServer);

            throw new RuntimeException('Unable to resolve runtime tool callback socket address.');

        }

        $authToken = \bin2hex(\random_bytes(16));
        $forkedProcessId = \pcntl_fork();

        if ($forkedProcessId === -1) {

            \fclose($socketServer);

            throw new RuntimeException('Unable to fork runtime tool callback server process.');

        }

        if ($forkedProcessId === 0) {

            $this->runRuntimeToolCallbackServer($socketServer, $runtimeToolsByName, $authToken);
            exit(0);

        }

        \fclose($socketServer);

        return [
            'pid' => $forkedProcessId,
            'endpoint' => "http://{$socketAddress}",
            'authToken' => $authToken,
        ];
    }

    /**
     * @param array{pid: int, endpoint: string, authToken: string} $runtimeToolCallbackServer
     */
    private function stopRuntimeToolCallbackServer(array $runtimeToolCallbackServer): void
    {
        $pid = $runtimeToolCallbackServer[ 'pid' ];

        if (\function_exists('posix_kill')) {
            @\posix_kill($pid, SIGTERM);
        }

        if (!\function_exists('pcntl_waitpid')) {
            return;
        }

        $startTime = \microtime(true);

        do {

            $waitResult = @\pcntl_waitpid($pid, $status, WNOHANG);

            if ($waitResult === $pid) {
                return;
            }

            \usleep(10_000);

        } while ((\microtime(true) - $startTime) < 2.0);

        if (\function_exists('posix_kill')) {
            @\posix_kill($pid, SIGKILL);
        }

        @\pcntl_waitpid($pid, $status);
    }

    /**
     * @param resource $socketServer
     * @param array<string, array{tool: Tool, bounded: array<string, mixed>}> $runtimeToolsByName
     */
    private function runRuntimeToolCallbackServer($socketServer, array $runtimeToolsByName, string $authToken): void
    {
        if (\function_exists('pcntl_async_signals')) {
            \pcntl_async_signals(true);
        }

        $running = true;

        if (\function_exists('pcntl_signal')) {

            \pcntl_signal(SIGTERM, static function () use (&$running): void {
                $running = false;
            });

            \pcntl_signal(SIGINT, static function () use (&$running): void {
                $running = false;
            });

        }

        while ($running) {

            $connection = @\stream_socket_accept($socketServer, 1);

            if (!\is_resource($connection)) {
                continue;
            }

            $responseBody = $this->handleRuntimeToolCallbackConnection($connection, $runtimeToolsByName, $authToken);
            $this->writeRuntimeToolCallbackHttpResponse($connection, $responseBody);
            \fclose($connection);

        }

        \fclose($socketServer);
    }

    /**
     * @param resource $connection
     * @param array<string, array{tool: Tool, bounded: array<string, mixed>}> $runtimeToolsByName
     *
     * @return array<string, mixed>
     */
    private function handleRuntimeToolCallbackConnection($connection, array $runtimeToolsByName, string $authToken): array
    {
        try {

            $request = $this->readRuntimeToolCallbackHttpRequest($connection);

        } catch (Throwable $throwable) {

            return $this->runtimeToolInvocationFailed(
                code: 'execution_failed',
                message: 'Failed to parse callback request: ' . $throwable->getMessage(),
            );

        }

        $providedAuthToken = $request[ 'headers' ][ 'x-engine-ai-tool-callback-token' ] ?? null;

        if (!\is_string($providedAuthToken) || $providedAuthToken !== $authToken) {

            return $this->runtimeToolInvocationFailed(
                code: 'execution_failed',
                message: 'Unauthorized runtime tool callback request.',
            );

        }

        $payload = \json_decode($request[ 'body' ], true);

        if (!\is_array($payload)) {

            return $this->runtimeToolInvocationFailed(
                code: 'invalid_arguments',
                message: 'Runtime tool callback payload must be a JSON object.',
            );

        }

        $toolName = \is_string($payload[ 'tool_name' ] ?? null) ? $payload[ 'tool_name' ] : '';
        $executionId = \is_string($payload[ 'execution_id' ] ?? null) ? $payload[ 'execution_id' ] : '';
        $invocationId = \is_string($payload[ 'invocation_id' ] ?? null) ? $payload[ 'invocation_id' ] : '';

        if ($toolName === '') {

            return $this->runtimeToolInvocationFailed(
                code: 'invalid_arguments',
                message: 'Runtime tool callback payload is missing `tool_name`.',
            );

        }

        $runtimeToolEntry = $runtimeToolsByName[ $toolName ] ?? null;

        if (!\is_array($runtimeToolEntry) || !$runtimeToolEntry[ 'tool' ] instanceof Tool) {

            return $this->runtimeToolInvocationFailed(
                code: 'tool_not_found',
                message: "No runtime tool handler registered for `{$toolName}`.",
            );

        }

        $inputArguments = \is_array($payload[ 'arguments' ] ?? null) ? $payload[ 'arguments' ] : [];
        $executionContext = \is_array($payload[ 'execution_context' ] ?? null) ? $payload[ 'execution_context' ] : [];
        $boundArguments = \is_array($executionContext[ 'bound_arguments' ] ?? null) ? $executionContext[ 'bound_arguments' ] : [];
        $boundedArguments = \array_merge($runtimeToolEntry[ 'bounded' ], $boundArguments);

        try {

            $toolOutput = $runtimeToolEntry[ 'tool' ]->invoke(new ToolData(
                input: $inputArguments,
                bounded: $boundedArguments,
                context: $executionContext,
                inputType: $runtimeToolEntry[ 'tool' ]->inputType(),
                boundedType: $runtimeToolEntry[ 'tool' ]->boundedType(),
            ));

        } catch (Throwable $throwable) {

            return $this->runtimeToolInvocationFailed(
                code: 'execution_failed',
                message: $throwable->getMessage(),
            );

        }

        return [
            'status' => 'succeeded',
            'result' => [
                'execution_id' => $executionId,
                'invocation_id' => $invocationId,
                'output' => $toolOutput,
            ],
        ];
    }

    /**
     * @param resource $connection
     *
     * @return array{method: string, path: string, headers: array<string, string>, body: string}
     */
    private function readRuntimeToolCallbackHttpRequest($connection): array
    {
        $rawRequest = '';

        while (!\str_contains($rawRequest, "\r\n\r\n")) {

            $chunk = \fread($connection, 8192);

            if ($chunk === false || $chunk === '') {
                break;
            }

            $rawRequest .= $chunk;

            if (\strlen($rawRequest) > 1_048_576) {
                throw new RuntimeException('Request headers are too large.');
            }

        }

        if (!\str_contains($rawRequest, "\r\n\r\n")) {
            throw new RuntimeException('Invalid HTTP request received.');
        }

        [ $headerBlock, $body ] = \explode("\r\n\r\n", $rawRequest, 2);
        $headerLines = \explode("\r\n", $headerBlock);
        $requestLine = \array_shift($headerLines);

        if (!\is_string($requestLine) || $requestLine === '') {
            throw new RuntimeException('Missing HTTP request line.');
        }

        $requestLineParts = \preg_split('/\s+/', $requestLine);

        if (!\is_array($requestLineParts) || \count($requestLineParts) < 2) {
            throw new RuntimeException('Invalid HTTP request line.');
        }

        $headers = [];

        foreach ($headerLines as $headerLine) {

            $headerParts = \explode(':', $headerLine, 2);

            if (\count($headerParts) !== 2) {
                continue;
            }

            $headerName = \strtolower(\trim($headerParts[ 0 ]));
            $headerValue = \trim($headerParts[ 1 ]);

            if ($headerName !== '') {
                $headers[ $headerName ] = $headerValue;
            }

        }

        $contentLength = isset($headers[ 'content-length' ]) ? (int) $headers[ 'content-length' ] : 0;

        while (\strlen($body) < $contentLength) {

            $chunk = \fread($connection, $contentLength - \strlen($body));

            if ($chunk === false || $chunk === '') {
                break;
            }

            $body .= $chunk;

        }

        return [
            'method' => (string) $requestLineParts[ 0 ],
            'path' => (string) $requestLineParts[ 1 ],
            'headers' => $headers,
            'body' => $body,
        ];
    }

    /**
     * @param resource $connection
     * @param array<string, mixed> $responseBody
     */
    private function writeRuntimeToolCallbackHttpResponse($connection, array $responseBody): void
    {
        $jsonPayload = \json_encode($responseBody);

        if (!\is_string($jsonPayload)) {
            $jsonPayload = '{"status":"failed","error":{"code":"internal","message":"Failed to encode callback response."}}';
        }

        $httpResponse = "HTTP/1.1 200 OK\r\n"
            . "Content-Type: application/json\r\n"
            . 'Content-Length: ' . \strlen($jsonPayload) . "\r\n"
            . "Connection: close\r\n\r\n"
            . $jsonPayload;

        @\fwrite($connection, $httpResponse);
    }

    /**
     * @return array{status: string, error: array{code: string, message: string}}
     */
    private function runtimeToolInvocationFailed(string $code, string $message): array
    {
        return [
            'status' => 'failed',
            'error' => [
                'code' => $code,
                'message' => $message,
            ],
        ];
    }
}
