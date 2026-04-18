<?php

declare(strict_types=1);

namespace Superwire\Laravel\Support;

use Illuminate\Contracts\Container\Container;
use RuntimeException;
use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\AgentToolCall;
use Superwire\Contracts\AgentToolResult;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Laravel\Contracts\WorkflowRuntimeTool;

final class LaravelRuntimeToolInvoker implements RuntimeToolInvokerInterface
{
    /**
     * @param array<string, class-string> $toolClassesByName
     */
    public function __construct(
        private readonly Container $container,
        private readonly array $toolClassesByName = [],
    ) {
    }

    /**
     * @param list<class-string> $toolClasses
     */
    public function withTools(array $toolClasses): self
    {
        $toolClassesByName = [];

        foreach ($toolClasses as $toolClass) {
            $toolClassesByName[$this->resolvedToolName($toolClass)] = $toolClass;
        }

        return new self($this->container, $toolClassesByName);
    }

    public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult
    {
        $toolClass = $this->toolClassesByName[$toolCall->name] ?? null;

        if ($toolClass === null) {
            return new AgentToolResult(
                toolCallId: $toolCall->id,
                toolName: $toolCall->name,
                arguments: $toolCall->arguments,
                result: "tool `{$toolCall->name}` is not registered for this workflow execution",
            );
        }

        $toolInstance = $this->container->make($toolClass);

        if (!$toolInstance instanceof WorkflowRuntimeTool) {
            throw new RuntimeException("configured tool class {$toolClass} must implement WorkflowRuntimeTool");
        }

        return new AgentToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            arguments: $toolCall->arguments,
            result: $toolInstance->invoke($this->toolBindings($request, $toolCall->name), $toolCall->arguments),
        );
    }

    /**
     * @return array<string, mixed>
     */
    private function toolBindings(AgentExecutionRequest $request, string $toolName): array
    {
        foreach ($request->tools as $toolExecution) {
            if ($toolExecution->name === $toolName) {
                return $toolExecution->bindings;
            }
        }

        return [];
    }

    private function resolvedToolName(string $toolClass): string
    {
        if (method_exists($toolClass, 'toolName')) {
            /** @var mixed $toolName */
            $toolName = $toolClass::toolName();

            if (is_string($toolName) && $toolName !== '') {
                return $toolName;
            }
        }

        $classBaseName = $toolClass;

        if (str_contains($toolClass, '\\')) {
            $segments = explode('\\', $toolClass);
            $classBaseName = (string) end($segments);
        }

        return strtolower((string) preg_replace('/(?<!^)[A-Z]/', '_$0', $classBaseName));
    }
}
