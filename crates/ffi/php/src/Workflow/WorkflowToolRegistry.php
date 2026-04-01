<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use InvalidArgumentException;
use ReflectionClass;
use Spatie\LaravelData\Data;

final class WorkflowToolRegistry extends Data
{
    /**
     * Runtime scoped tools keyed by tool name.
     *
     * @var array<string, Tool>
     */
    public array $toolsByName;

    /**
     * Tool declarations sent to the engine request envelope.
     *
     * @var array<int, array<string, mixed>>
     */
    public array $toolDeclarations;

    /**
     * @param array<string, Tool> $toolsByName
     * @param array<int, array<string, mixed>> $toolDeclarations
     */
    private function __construct(array $toolsByName, array $toolDeclarations)
    {
        $this->toolsByName = $toolsByName;
        $this->toolDeclarations = $toolDeclarations;
    }

    /**
     * Builds a normalized tool registry from workflow tool definitions.
     *
     * @param array<int, class-string<Tool>|Tool> $tools
     */
    public static function fromList(array $tools): self
    {
        if (!\array_is_list($tools)) {
            throw new InvalidArgumentException('Workflow `tools` must be a list.');
        }

        $toolsByName = [];
        $toolDeclarations = [];

        foreach ($tools as $toolOrClass) {

            $tool = self::resolveTool($toolOrClass);
            $toolsByName[ $tool->name ] = $tool;
            $toolDeclarations[] = $tool->toDeclaration();

        }

        return new self($toolsByName, $toolDeclarations);
    }

    /**
     * Returns scoped tools indexed by tool name.
     *
     * @return array<string, Tool>
     */
    public function byName(): array
    {
        return $this->toolsByName;
    }

    /**
     * Returns serialized declarations for all scoped tools.
     *
     * @return array<int, array<string, mixed>>
     */
    public function declarations(): array
    {
        return $this->toolDeclarations;
    }

    private static function resolveTool(mixed $toolOrClass): Tool
    {
        if ($toolOrClass instanceof Tool) {
            return $toolOrClass;
        }

        if (is_string($toolOrClass)) {
            return self::instantiateTool($toolOrClass);
        }

        throw new InvalidArgumentException('Every workflow tool must be a Tool instance or a Tool class-string.');
    }

    private static function instantiateTool(string $toolClass): Tool
    {
        if ($toolClass === '') {
            throw new InvalidArgumentException('Workflow tool class-string must not be empty.');
        }

        if (!\class_exists($toolClass)) {
            throw new InvalidArgumentException("Workflow tool class `{$toolClass}` does not exist.");
        }

        if (!\is_subclass_of($toolClass, Tool::class)) {
            throw new InvalidArgumentException("Workflow tool class `{$toolClass}` must extend `" . Tool::class . '`');
        }

        $reflectionClass = new ReflectionClass($toolClass);

        if (!$reflectionClass->isInstantiable()) {
            throw new InvalidArgumentException("Workflow tool class `{$toolClass}` is not instantiable.");
        }

        $constructor = $reflectionClass->getConstructor();

        if ($constructor !== null && $constructor->getNumberOfRequiredParameters() > 0) {

            throw new InvalidArgumentException(
                "Workflow tool class `{$toolClass}` constructor must not require parameters when passed as a class-string.",
            );

        }

        return $reflectionClass->newInstance();
    }
}
