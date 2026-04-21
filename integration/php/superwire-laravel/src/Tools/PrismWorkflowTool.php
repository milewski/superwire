<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Tool;

final readonly class PrismWorkflowTool implements WorkflowTool
{
    public function __construct(
        private Tool $tool,
    )
    {
    }

    public function name(): string
    {
        return $this->tool->name();
    }

    public function toPrismTool(array $boundArguments = []): Tool
    {
        return $this->tool;
    }
}
