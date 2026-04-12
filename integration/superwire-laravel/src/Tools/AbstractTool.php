<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Illuminate\Support\Str;
use InvalidArgumentException;
use ReflectionException;
use Spatie\LaravelData\Data;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Tools\Execution\ToolDataMapper;
use Superwire\Laravel\Tools\Execution\ToolExecutionSignature;
use Superwire\Laravel\Tools\Execution\ToolExecutionSignatureFactory;
use Superwire\Laravel\Tools\Execution\ToolExecutionSignatureRegistry;
use Superwire\Laravel\Tools\Execution\ToolHandleParameter;
use Superwire\Laravel\Tools\Execution\ToolHandleParameterKind;
use Swaggest\JsonSchema\Schema;

abstract class AbstractTool implements Tool
{
    private static ?ToolExecutionSignatureRegistry $executionSignatures = null;
    private static ?ToolExecutionSignatureFactory $executionSignatureFactory = null;
    private static ?ToolDataMapper $toolDataMapper = null;

    public static function name(): string
    {
        return Str::snake(class_basename(static::class));
    }

    public static function description(): string
    {
        return sprintf('Proxy tool for %s', static::class);
    }

    public static function endpointName(): string
    {
        return static::name();
    }

    /**
     * @return class-string<ToolInputData>
     */
    final public static function agentInputClass(): string
    {
        return static::executionSignature()->agentInputClass;
    }

    /**
     * @return class-string<ToolBoundInputData>
     */
    final public static function boundInputClass(): string
    {
        return static::executionSignature()->boundInputClass;
    }

    /**
     * @return class-string<Data>
     */
    final public static function outputClass(): string
    {
        return static::executionSignature()->outputClass;
    }

    final public static function inputSchema(): Schema
    {
        $toolDataMapper = self::$toolDataMapper ??= new ToolDataMapper();

        return $toolDataMapper->schemaFromToolDataClass(static::agentInputClass());
    }

    final public static function boundInputSchema(): Schema
    {
        $toolDataMapper = self::$toolDataMapper ??= new ToolDataMapper();

        return $toolDataMapper->schemaFromToolDataClass(static::boundInputClass());
    }

    final public static function outputSchema(): Schema
    {
        $toolDataMapper = self::$toolDataMapper ??= new ToolDataMapper();

        return $toolDataMapper->schemaFromToolDataClass(static::outputClass());
    }

    final public static function resolveAgentInput(array $agentInputPayload): ToolInputData
    {
        $executionSignature = static::executionSignature();
        $toolDataMapper = self::$toolDataMapper ??= new ToolDataMapper();
        $resolvedAgentInput = $toolDataMapper->hydrateToolDataClass(
            $executionSignature->agentInputClass,
            $agentInputPayload,
            'agent input',
        );

        if ($resolvedAgentInput instanceof $executionSignature->agentInputClass) {
            return $resolvedAgentInput;
        }

        throw new InvalidArgumentException(sprintf(
            'agent input must resolve to `%s`, received `%s`',
            $executionSignature->agentInputClass,
            $resolvedAgentInput::class,
        ));
    }

    /**
     * @param array<string, mixed> $boundInputPayload
     */
    final public static function resolveBoundInput(array $boundInputPayload): ToolBoundInputData
    {
        $executionSignature = static::executionSignature();
        $toolDataMapper = self::$toolDataMapper ??= new ToolDataMapper();
        $resolvedBoundInput = $toolDataMapper->hydrateToolDataClass(
            $executionSignature->boundInputClass,
            $boundInputPayload,
            'bound input',
        );

        if ($resolvedBoundInput instanceof $executionSignature->boundInputClass) {
            return $resolvedBoundInput;
        }

        throw new InvalidArgumentException(sprintf(
            'bound input must resolve to `%s`, received `%s`',
            $executionSignature->boundInputClass,
            $resolvedBoundInput::class,
        ));
    }

    /**
     * @throws ReflectionException
     * @return array<string, mixed>
     */
    final public function execute(ToolInputData $agentInput, ToolBoundInputData $boundInput): array
    {
        $executionSignature = static::executionSignature();

        if (!$agentInput instanceof $executionSignature->agentInputClass) {

            throw new InvalidArgumentException(sprintf(
                'agent input must be `%s`, received `%s`',
                $executionSignature->agentInputClass,
                $agentInput::class,
            ));

        }

        if (!$boundInput instanceof $executionSignature->boundInputClass) {

            throw new InvalidArgumentException(sprintf(
                'bound input must be `%s`, received `%s`',
                $executionSignature->boundInputClass,
                $boundInput::class,
            ));

        }

        $handleArguments = $this->handleArgumentsForExecution(
            $executionSignature,
            $agentInput,
            $boundInput,
        );

        $toolOutput = $this->handle(...$handleArguments);

        if (!$toolOutput instanceof $executionSignature->outputClass) {

            throw new InvalidArgumentException(sprintf(
                'tool `%s` handle method must return `%s`, received `%s`',
                static::class,
                $executionSignature->outputClass,
                $toolOutput::class,
            ));

        }

        $toolDataMapper = self::$toolDataMapper ??= new ToolDataMapper();

        return $toolDataMapper->extractToolDataPayload($toolOutput);
    }

    /**
     * @return list<mixed>
     */
    private function handleArgumentsForExecution(
        ToolExecutionSignature $executionSignature,
        ToolInputData $agentInput,
        ToolBoundInputData $boundInput,
    ): array {
        $handleArguments = [];

        foreach ($executionSignature->handleParameters() as $handleParameter) {
            $handleArguments[] = $this->handleArgumentValue($handleParameter, $agentInput, $boundInput);
        }

        return $handleArguments;
    }

    private function handleArgumentValue(
        ToolHandleParameter $handleParameter,
        ToolInputData $agentInput,
        ToolBoundInputData $boundInput,
    ): mixed {
        return match ($handleParameter->kind) {
            ToolHandleParameterKind::AgentInput => $agentInput,
            ToolHandleParameterKind::BoundInput => $boundInput,
            ToolHandleParameterKind::Container => app($handleParameter->className),
        };
    }

    /**
     * Each tool implementation must define:
     *
     * `protected function handle(MyInput $agentInput, MyBoundInput $boundInput): MyOutput`
     *
     * where input and bound DTO classes implement ToolInputData and ToolBoundInputData.
     * The output DTO class must extend Laravel Data.
     */
    private static function executionSignature(): ToolExecutionSignature
    {
        $executionSignatures = self::$executionSignatures ??= new ToolExecutionSignatureRegistry();
        $executionSignatureFactory = self::$executionSignatureFactory ??= new ToolExecutionSignatureFactory();

        return $executionSignatures->remember(
            static::class,
            static fn (): ToolExecutionSignature => $executionSignatureFactory->build(static::class),
        );
    }
}
