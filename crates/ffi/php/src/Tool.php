<?php

declare(strict_types=1);

namespace EngineAi\Ffi;

use RuntimeException;

abstract class Tool
{
    public readonly string $name;

    public function __construct(?string $name = null)
    {
        $this->name = $name ?? $this->resolveToolName();
    }

    abstract public function description(): string;

    abstract public function inputSchema(): array;

    abstract public function execute(array $toolArguments): mixed;

    public function outputSchema(): ?array
    {
        return null;
    }

    public function toDeclaration(): array
    {
        $declaration = [
            'name' => $this->name,
            'description' => $this->description(),
            'input_schema' => $this->inputSchema(),
        ];

        $outputSchema = $this->outputSchema();

        if ($outputSchema !== null) {
            $declaration['output_schema'] = $outputSchema;
        }

        return $declaration;
    }

    private function resolveToolName(): string
    {
        $toolClassName = static::class;
        $toolNameConstant = $toolClassName . '::TOOL_NAME';

        if (\defined($toolNameConstant)) {
            $staticToolName = \trim((string) \constant($toolNameConstant));

            if ($staticToolName !== '') {
                return $staticToolName;
            }
        }

        return $this->deriveToolNameFromClassName();
    }

    private function deriveToolNameFromClassName(): string
    {
        $fullyQualifiedClassName = static::class;
        $classNameSegments = \explode('\\', $fullyQualifiedClassName);
        $shortClassName = \trim((string) \end($classNameSegments));

        if ($shortClassName === '') {
            throw new RuntimeException('Tool name could not be inferred from class name.');
        }

        $normalizedName = \preg_replace('/([a-z0-9])([A-Z])/', '$1_$2', $shortClassName);
        $normalizedName = \preg_replace('/[-\s]+/', '_', (string) $normalizedName);
        $normalizedName = \strtolower((string) $normalizedName);

        if ($normalizedName === '') {
            throw new RuntimeException('Tool name normalization produced an empty value.');
        }

        return $normalizedName;
    }
}
