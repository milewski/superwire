<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

final class ToolExecutionSignatureRegistry
{
    /**
     * @var array<class-string, ToolExecutionSignature>
     */
    private array $executionSignatures = [];

    public function has(string $toolClassName): bool
    {
        return array_key_exists($toolClassName, $this->executionSignatures);
    }

    public function get(string $toolClassName): ?ToolExecutionSignature
    {
        return $this->executionSignatures[ $toolClassName ] ?? null;
    }

    /**
     * @param class-string $toolClassName
     */
    public function set(string $toolClassName, ToolExecutionSignature $executionSignature): void
    {
        $this->executionSignatures[ $toolClassName ] = $executionSignature;
    }

    /**
     * @param class-string $toolClassName
     * @param callable(): ToolExecutionSignature $signatureResolver
     */
    public function remember(string $toolClassName, callable $signatureResolver): ToolExecutionSignature
    {
        $existingExecutionSignature = $this->get($toolClassName);

        if ($existingExecutionSignature !== null) {
            return $existingExecutionSignature;
        }

        $resolvedExecutionSignature = $signatureResolver();

        $this->set($toolClassName, $resolvedExecutionSignature);

        return $resolvedExecutionSignature;
    }
}
