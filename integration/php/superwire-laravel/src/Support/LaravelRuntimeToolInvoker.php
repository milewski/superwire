<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Support;

use Illuminate\Contracts\Container\Container;
use ReflectionIntersectionType;
use ReflectionMethod;
use ReflectionNamedType;
use ReflectionParameter;
use ReflectionType;
use ReflectionUnionType;
use RuntimeException;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Contracts\RuntimeToolInvokerInterface;
use Superwire\Contracts\Contracts\RuntimeToolMetadataProviderInterface;
use Superwire\Contracts\Contracts\RuntimeToolSchemaProviderInterface;
use Superwire\Laravel\Contracts\WorkflowRuntimeTool;
use Superwire\Laravel\Tools\WorkflowToolArguments;
use Superwire\Laravel\Tools\WorkflowToolInput;
use Superwire\Laravel\Tools\WorkflowToolResult;
use Swaggest\JsonSchema\Schema;
use Throwable;

final class LaravelRuntimeToolInvoker implements RuntimeToolInvokerInterface, RuntimeToolMetadataProviderInterface, RuntimeToolSchemaProviderInterface
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
            $toolClassesByName[ $this->resolvedToolName($toolClass) ] = $toolClass;
        }

        return new self($this->container, $toolClassesByName);
    }

    public function invoke(AgentExecutionRequest $request, AgentToolCall $toolCall): AgentToolResult
    {
        $toolClass = $this->toolClassesByName[ $toolCall->name ] ?? null;

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

        if (!method_exists($toolInstance, 'invoke')) {
            throw new RuntimeException("configured tool class {$toolClass} must define invoke()");
        }

        try {

            $toolResult = $this->invokeTool($toolInstance, $this->toolBindings($request, $toolCall->name), $toolCall->arguments);

        } catch (Throwable $throwable) {

            $toolResult = WorkflowToolResult::fail('runtime tool invocation failed', [
                'tool' => $toolCall->name,
                'reason' => $throwable->getMessage(),
            ]);

        }

        return new AgentToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            arguments: $toolCall->arguments,
            result: $toolResult,
        );
    }

    public function schemaForTool(string $toolName): ?Schema
    {
        $toolClass = $this->toolClassesByName[ $toolName ] ?? null;

        if ($toolClass === null || !method_exists($toolClass, 'invoke')) {
            return null;
        }

        $invokeMethod = new ReflectionMethod($toolClass, 'invoke');
        $invokeParameters = $invokeMethod->getParameters();

        if (!array_key_exists(1, $invokeParameters)) {
            return null;
        }

        $inputClassName = $this->resolveWorkflowToolArgumentClassName($invokeParameters[ 1 ]->getType(), WorkflowToolInput::class);

        if ($inputClassName === null) {
            return null;
        }

        return $inputClassName::schema();
    }

    public function descriptionForTool(string $toolName): ?string
    {
        $toolClass = $this->toolClassesByName[ $toolName ] ?? null;

        if ($toolClass === null) {
            return null;
        }

        if (!method_exists($toolClass, 'toolDescription')) {
            return null;
        }

        $toolDescription = $toolClass::toolDescription();

        if (!is_string($toolDescription) || trim($toolDescription) === '') {
            return null;
        }

        return $toolDescription;
    }

    public function strictSchemaForTool(string $toolName): ?bool
    {
        $toolClass = $this->toolClassesByName[ $toolName ] ?? null;

        if ($toolClass === null) {
            return null;
        }

        if (!method_exists($toolClass, 'toolStrictSchema')) {
            return true;
        }

        $strictSchema = $toolClass::toolStrictSchema();

        if (!is_bool($strictSchema)) {
            return true;
        }

        return $strictSchema;
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

    /**
     * @param array<string, mixed> $boundArguments
     * @param array<string, mixed> $agentArguments
     */
    private function invokeTool(WorkflowRuntimeTool $toolInstance, array $boundArguments, array $agentArguments): mixed
    {
        $invokeMethod = new ReflectionMethod($toolInstance, 'invoke');
        $invokeParameters = $invokeMethod->getParameters();
        $resolvedArguments = [];

        foreach ($invokeParameters as $parameterIndex => $invokeParameter) {

            if ($parameterIndex === 0) {

                $resolvedArguments[] = $this->resolveArgumentValue($invokeParameter, $boundArguments, $toolInstance::class, 'bound arguments');

                continue;

            }

            if ($parameterIndex === 1) {

                $resolvedArguments[] = $this->resolveArgumentValue($invokeParameter, $agentArguments, $toolInstance::class, 'tool arguments');

                continue;

            }

            $resolvedArguments[] = $this->resolveContainerDependency($invokeParameter, $toolInstance::class);

        }

        return $invokeMethod->invokeArgs($toolInstance, $resolvedArguments);
    }

    /**
     * @param array<string, mixed> $payload
     */
    private function resolveArgumentValue(ReflectionParameter $parameter, array $payload, string $toolClass, string $payloadDescription): mixed
    {
        if ($payload === [] && $parameter->isDefaultValueAvailable()) {
            return $parameter->getDefaultValue();
        }

        $argumentClassName = $this->resolveWorkflowToolArgumentClassName($parameter->getType(), WorkflowToolArguments::class);

        if ($argumentClassName !== null) {

            if ($payload === [] && $parameter->allowsNull()) {
                return null;
            }

            try {

                return $argumentClassName::fromPayload($payload);

            } catch (Throwable $throwable) {

                throw new RuntimeException(
                    "failed to resolve {$payloadDescription} for `{$toolClass}` parameter `{$parameter->getName()}`: {$throwable->getMessage()}",
                    previous: $throwable,
                );

            }

        }

        return $payload;
    }

    private function resolveContainerDependency(ReflectionParameter $parameter, string $toolClass): mixed
    {
        $reflectionType = $parameter->getType();

        if ($reflectionType instanceof ReflectionNamedType && !$reflectionType->isBuiltin()) {
            return $this->container->make($reflectionType->getName());
        }

        if ($parameter->isDefaultValueAvailable()) {
            return $parameter->getDefaultValue();
        }

        throw new RuntimeException("unable to resolve parameter `{$parameter->getName()}` for `{$toolClass}` invoke()");
    }

    /**
     * @param class-string<WorkflowToolArguments> $expectedBaseClass
     * @return class-string<WorkflowToolArguments>|null
     */
    private function resolveWorkflowToolArgumentClassName(?ReflectionType $reflectionType, string $expectedBaseClass): ?string
    {
        if ($reflectionType === null || $reflectionType instanceof ReflectionIntersectionType) {
            return null;
        }

        if ($reflectionType instanceof ReflectionUnionType) {

            foreach ($reflectionType->getTypes() as $unionedType) {

                if ($unionedType->isBuiltin()) {
                    continue;
                }

                $argumentClassName = $unionedType->getName();

                if (is_subclass_of($argumentClassName, $expectedBaseClass)) {
                    return $argumentClassName;
                }

            }

            return null;

        }

        if ($reflectionType->isBuiltin()) {
            return null;
        }

        $argumentClassName = $reflectionType->getName();

        if (is_subclass_of($argumentClassName, $expectedBaseClass)) {
            return $argumentClassName;
        }

        return null;
    }
}
