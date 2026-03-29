<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use InvalidArgumentException;

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
        $providedBridge = $options['bridge'] ?? null;

        if ($providedBridge !== null && !$providedBridge instanceof EngineFfiBridge) {
            throw new InvalidArgumentException('Engine `bridge` must be an EngineFfiBridge instance when provided.');
        }

        $this->engineFfiBridge = $providedBridge ?? new EngineFfiBridge($options['bridgeOptions'] ?? []);

        $providedExecutionIdGenerator = $options['executionIdGenerator'] ?? null;

        if ($providedExecutionIdGenerator !== null && !\is_callable($providedExecutionIdGenerator)) {
            throw new InvalidArgumentException('Engine `executionIdGenerator` must be callable when provided.');
        }

        $this->executionIdGenerator = $providedExecutionIdGenerator ?? fn (): string => $this->generateExecutionId();
        $this->registeredToolsByName = [];
    }

    public function registerGlobalTool(Tool $tool, array $options = []): self
    {
        $boundedArguments = \is_array($options['bounded'] ?? null) ? $options['bounded'] : [];

        $this->registeredToolsByName[$tool->name] = [
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

        unset($this->registeredToolsByName[$toolName]);

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
            $registeredTools[] = $registeredToolEntry['tool'];
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
        $registeredTool = $this->registeredToolsByName[$toolName] ?? null;

        if (!\is_array($registeredTool) || !$registeredTool['tool'] instanceof Tool) {
            throw new \RuntimeException("Tool `{$toolName}` is not registered. Call engine->registerGlobalTool(...) first.");
        }

        return $registeredTool['tool']->execute([
            'input' => $input,
            'bounded' => $registeredTool['bounded'],
            'context' => [],
        ]);
    }

    public function run(Workflow $workflow): EngineExecutionResult
    {
        $executionId = '';

        try {
            $defaultExecutionId = ($this->executionIdGenerator)();

            if (!\is_string($defaultExecutionId) || $defaultExecutionId === '') {
                throw new \RuntimeException('Execution ID generator must return a non-empty string.');
            }

            $executionId = $workflow->executionId($defaultExecutionId);
            $workflowExecutionRequest = $workflow->toExecutionRequest($executionId);
            $workflowExecutionRequest['custom_tools'] = $this->resolveCustomToolDeclarations($workflowExecutionRequest['custom_tools']);
            $workflowExecutionRequest['defer_output'] = true;

            if ($this->hasRuntimeTools($workflow)) {
                return new EngineExecutionResult(
                    $this->engineFfiBridge,
                    $executionId,
                    [
                        'code' => 'execution_failed',
                        'message' => 'In-process PHP tool callbacks are not supported yet. Use declarative custom tool schemas with an external `tool_callback` endpoint.',
                    ],
                    false,
                );
            }

            $workflowExecutionEnvelope = $this->engineFfiBridge->executeWorkflow($workflowExecutionRequest, [
                'requestId' => $workflow->requestId(),
            ]);

            if (($workflowExecutionEnvelope['status'] ?? null) === 'failed') {
                $workflowError = \is_array($workflowExecutionEnvelope['error'] ?? null) ? $workflowExecutionEnvelope['error'] : [];

                return new EngineExecutionResult(
                    $this->engineFfiBridge,
                    $executionId,
                    [
                        'code' => \is_string($workflowError['code'] ?? null) ? $workflowError['code'] : 'execution_failed',
                        'message' => \is_string($workflowError['message'] ?? null)
                            ? $workflowError['message']
                            : 'Unknown workflow execution error.',
                        'context' => $workflowError['context'] ?? null,
                        'details' => $workflowError['details'] ?? null,
                    ],
                    true,
                );
            }

            $outputEnvelope = \is_array($workflowExecutionEnvelope['output'] ?? null)
                ? $workflowExecutionEnvelope['output']
                : [];
            $resultExecutionId = \is_string($outputEnvelope['execution_id'] ?? null) ? $outputEnvelope['execution_id'] : $executionId;

            return new EngineExecutionResult($this->engineFfiBridge, $resultExecutionId);
        } catch (\Throwable $throwable) {
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
            $customToolDeclarationsByName[$registeredTool->name] = $registeredTool->toDeclaration();
        }

        foreach ($workflowDeclaredTools as $customToolDeclaration) {
            if (!\is_array($customToolDeclaration) || !\is_string($customToolDeclaration['name'] ?? null)) {
                continue;
            }

            $customToolDeclarationsByName[$customToolDeclaration['name']] = $customToolDeclaration;
        }

        return \array_values($customToolDeclarationsByName);
    }

    private function hasRuntimeTools(Workflow $workflow): bool
    {
        return $this->registeredToolsByName !== [] || $workflow->scopedTools() !== [];
    }
}
