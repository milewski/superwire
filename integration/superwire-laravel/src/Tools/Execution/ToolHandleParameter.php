<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Execution;

final readonly class ToolHandleParameter
{
    /**
     * @param class-string $className
     */
    public function __construct(
        public ToolHandleParameterKind $kind,
        public string $className,
    ) {
    }

    /**
     * @param class-string $className
     */
    public static function agentInput(string $className): self
    {
        return new self(ToolHandleParameterKind::AgentInput, $className);
    }

    /**
     * @param class-string $className
     */
    public static function boundInput(string $className): self
    {
        return new self(ToolHandleParameterKind::BoundInput, $className);
    }

    /**
     * @param class-string $className
     */
    public static function container(string $className): self
    {
        return new self(ToolHandleParameterKind::Container, $className);
    }
}
