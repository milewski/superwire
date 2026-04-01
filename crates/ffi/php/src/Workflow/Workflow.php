<?php

declare(strict_types = 1);

namespace EngineAi\Ffi;

use InvalidArgumentException;
use ReflectionClass;
use RuntimeException;

class Workflow
{
    public readonly string $source;

    public readonly array $inputPayload;

    public readonly ?array $secretsPayload;

    public readonly array $customTools;

    /**
     * @var array<string, Tool>
     */
    private array $scopedToolsByName;

    /**
     * @var array{requestId: string|null, executionId: string|null}
     */
    private array $runOptions;

    public function __construct(
        string $source,
        array $inputs = [],
        ?array $secrets = null,
        array $tools = [],
        ?string $requestId = null,
        ?string $executionId = null,
    )
    {
        $options = self::resolveWorkflowOptions($inputs, $secrets, $tools, $requestId, $executionId);
        $legacyRunOptions = \is_array($options[ 'options' ] ?? null) ? $options[ 'options' ] : [];
        $tools = $this->normalizeTools($options[ 'tools' ] ?? []);

        $this->source = $source;
        $this->inputPayload = \is_array($options[ 'inputs' ] ?? null) ? $options[ 'inputs' ] : [];
        $this->secretsPayload = \is_array($options[ 'secrets' ] ?? null) ? $options[ 'secrets' ] : null;
        $this->customTools = $this->resolveCustomTools($tools);
        $this->scopedToolsByName = $this->resolveScopedToolsByName($tools);
        $this->runOptions = [
            'requestId' => $this->normalizeNullableString($options[ 'requestId' ] ?? $legacyRunOptions[ 'requestId' ] ?? null),
            'executionId' => $this->normalizeNullableString($options[ 'executionId' ] ?? $legacyRunOptions[ 'executionId' ] ?? null),
        ];
    }

    public static function fromFile(
        string $path,
        array $inputs = [],
        ?array $secrets = null,
        array $tools = [],
        ?string $requestId = null,
        ?string $executionId = null,
    ): self
    {
        $resolvedPath = \realpath($path);

        if ($resolvedPath === false) {
            throw new RuntimeException("Workflow file does not exist: {$path}");
        }

        $source = \file_get_contents($resolvedPath);

        if ($source === false) {
            throw new RuntimeException("Unable to read workflow file: {$resolvedPath}");
        }

        return new self($source, $inputs, $secrets, $tools, $requestId, $executionId);
    }

    /**
     * @param array<string, mixed> $inputs
     *
     * @return array<string, mixed>
     */
    private static function resolveWorkflowOptions(
        array $inputs,
        ?array $secrets,
        array $tools,
        ?string $requestId,
        ?string $executionId,
    ): array
    {
        if (
            $secrets === null
            && $tools === []
            && $requestId === null
            && $executionId === null
            && self::isLegacyWorkflowOptions($inputs)
        ) {
            return $inputs;
        }

        return [
            'inputs' => $inputs,
            'secrets' => $secrets,
            'tools' => $tools,
            'requestId' => $requestId,
            'executionId' => $executionId,
        ];
    }

    /**
     * @param array<string, mixed> $options
     */
    private static function isLegacyWorkflowOptions(array $options): bool
    {
        return \array_key_exists('inputs', $options)
            || \array_key_exists('secrets', $options)
            || \array_key_exists('tools', $options)
            || \array_key_exists('requestId', $options)
            || \array_key_exists('executionId', $options)
            || \array_key_exists('options', $options);
    }

    public function executionId(string $fallbackExecutionId): string
    {
        return $this->runOptions[ 'executionId' ] ?? $fallbackExecutionId;
    }

    public function requestId(): ?string
    {
        return $this->runOptions[ 'requestId' ];
    }

    /**
     * @return array<int, Tool>
     */
    public function scopedTools(): array
    {
        return \array_values($this->scopedToolsByName);
    }

    /**
     * @return array<string, Tool>
     */
    public function scopedToolsByName(): array
    {
        return $this->scopedToolsByName;
    }

    public function toExecutionRequest(string $executionId, ?array $inputPayload = null, ?array $secretsPayload = null): array
    {
        $workflowExecutionRequest = [
            'execution_id' => $executionId,
            'workflow_source' => $this->source,
            'input' => [
                'payload' => $inputPayload ?? $this->inputPayload,
            ],
            'custom_tools' => $this->customTools,
        ];

        $resolvedSecretsPayload = $secretsPayload ?? $this->secretsPayload;

        if ($resolvedSecretsPayload !== null) {

            $workflowExecutionRequest[ 'secrets' ] = [
                'payload' => $resolvedSecretsPayload,
            ];

        }

        return $workflowExecutionRequest;
    }

    /**
     * @param array<int, mixed> $tools
     *
     * @return array<int, Tool>
     */
    private function normalizeTools(array $tools): array
    {
        if (!\array_is_list($tools)) {
            throw new InvalidArgumentException('Workflow `tools` must be a list.');
        }

        $normalizedTools = [];

        foreach ($tools as $toolOrClass) {

            if ($toolOrClass instanceof Tool) {

                $normalizedTools[] = $toolOrClass;

                continue;

            }

            if (\is_string($toolOrClass)) {

                $normalizedTools[] = $this->instantiateTool($toolOrClass);

                continue;

            }

            throw new InvalidArgumentException('Every workflow tool must be a Tool instance or a Tool class-string.');

        }

        return $normalizedTools;
    }

    /**
     * @param array<int, Tool> $tools
     *
     * @return array<int, array>
     */
    private function resolveCustomTools(array $tools): array
    {
        $customTools = [];

        foreach ($tools as $tool) {
            $customTools[] = $tool->toDeclaration();
        }

        return $customTools;
    }

    /**
     * @param array<int, Tool> $tools
     *
     * @return array<string, Tool>
     */
    private function resolveScopedToolsByName(array $tools): array
    {
        $scopedToolsByName = [];

        foreach ($tools as $tool) {
            $scopedToolsByName[ $tool->name ] = $tool;
        }

        return $scopedToolsByName;
    }

    private function instantiateTool(string $toolClass): Tool
    {
        if ($toolClass === '') {
            throw new InvalidArgumentException('Workflow tool class-string must not be empty.');
        }

        if (!class_exists($toolClass)) {
            throw new InvalidArgumentException("Workflow tool class `{$toolClass}` does not exist.");
        }

        if (!is_subclass_of($toolClass, Tool::class)) {
            throw new InvalidArgumentException("Workflow tool class `{$toolClass}` must extend `" . Tool::class . '`.');
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

    private function normalizeNullableString(mixed $value): ?string
    {
        if ($value === null) {
            return null;
        }

        if (!\is_string($value)) {
            throw new InvalidArgumentException('Workflow run option values must be strings when provided.');
        }

        return $value;
    }
}
