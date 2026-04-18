<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Contracts;

interface WorkflowRuntimeTool
{
    /**
     * @param array<string, mixed> $boundArguments
     * @param array<string, mixed> $agentArguments
     */
    public function invoke(array $boundArguments, array $agentArguments = []): string;
}
