<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Testing;

use Prism\Prism\ValueObjects\ToolCall;
use Superwire\Laravel\Tools\WorkflowTool;

final class ToolCallFactory
{
    /**
     * @param class-string<WorkflowTool>|WorkflowTool $tool
     * @param array<string, mixed> $arguments
     */
    public static function fromClass(string|WorkflowTool $tool, array $arguments = [], ?string $id = null): ToolCall
    {
        return new ToolCall(
            id: $id ?? sprintf('fake-%s', $tool::name()),
            name: $tool::name(),
            arguments: $arguments,
        );
    }
}
