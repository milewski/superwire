<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use InvalidArgumentException;
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

    public function __construct(string $source, array $options = [])
    {
        $legacyRunOptions = \is_array($options['options'] ?? null) ? $options['options'] : [];
        $tools = $this->normalizeTools($options['tools'] ?? []);

        $this->source = $source;
        $this->inputPayload = \is_array($options['inputs'] ?? null) ? $options['inputs'] : [];
        $this->secretsPayload = \is_array($options['secrets'] ?? null) ? $options['secrets'] : null;
        $this->customTools = $this->resolveCustomTools($tools);
        $this->scopedToolsByName = $this->resolveScopedToolsByName($tools);
        $this->runOptions = [
            'requestId' => $this->normalizeNullableString($options['requestId'] ?? $legacyRunOptions['requestId'] ?? null),
            'executionId' => $this->normalizeNullableString($options['executionId'] ?? $legacyRunOptions['executionId'] ?? null),
        ];
    }

    public static function fromFile(string $filePath, array $options = []): self
    {
        $resolvedPath = \realpath($filePath);

        if ($resolvedPath === false) {
            throw new RuntimeException("Workflow file does not exist: {$filePath}");
        }

        $source = \file_get_contents($resolvedPath);

        if ($source === false) {
            throw new RuntimeException("Unable to read workflow file: {$resolvedPath}");
        }

        return new self($source, $options);
    }

    public function executionId(string $fallbackExecutionId): string
    {
        return $this->runOptions['executionId'] ?? $fallbackExecutionId;
    }

    public function requestId(): ?string
    {
        return $this->runOptions['requestId'];
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
            $workflowExecutionRequest['secrets'] = [
                'payload' => $resolvedSecretsPayload,
            ];
        }

        return $workflowExecutionRequest;
    }

    /**
     * @param array<int, mixed> $tools
     *
     * @return array<int, Tool|array>
     */
    private function normalizeTools(array $tools): array
    {
        if (!\array_is_list($tools)) {
            throw new InvalidArgumentException('Workflow `tools` must be a list.');
        }

        return $tools;
    }

    /**
     * @param array<int, Tool|array> $tools
     *
     * @return array<int, array>
     */
    private function resolveCustomTools(array $tools): array
    {
        $customTools = [];

        foreach ($tools as $toolOrDeclaration) {
            if ($toolOrDeclaration instanceof Tool) {
                $customTools[] = $toolOrDeclaration->toDeclaration();

                continue;
            }

            if (!\is_array($toolOrDeclaration)) {
                throw new InvalidArgumentException('Every workflow tool must be a Tool instance or a declaration array.');
            }

            $customTools[] = $toolOrDeclaration;
        }

        return $customTools;
    }

    /**
     * @param array<int, Tool|array> $tools
     *
     * @return array<string, Tool>
     */
    private function resolveScopedToolsByName(array $tools): array
    {
        $scopedToolsByName = [];

        foreach ($tools as $toolOrDeclaration) {
            if (!$toolOrDeclaration instanceof Tool) {
                continue;
            }

            $scopedToolsByName[$toolOrDeclaration->name] = $toolOrDeclaration;
        }

        return $scopedToolsByName;
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
