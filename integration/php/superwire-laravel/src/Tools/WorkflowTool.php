<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Tool;

interface WorkflowTool
{
    public static function name(): string;

    public function toPrismTool(array $boundArguments = []): Tool;
}
