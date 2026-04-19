<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Superwire\Laravel\Contracts\WorkflowRuntimeTool;

abstract class WorkflowTool implements WorkflowRuntimeTool
{
    public static function toolName(): string
    {
        $className = static::class;
        $classBaseName = $className;

        if (str_contains($className, '\\')) {

            $segments = explode('\\', $className);
            $classBaseName = (string) end($segments);

        }

        return strtolower((string) preg_replace('/(?<!^)[A-Z]/', '_$0', $classBaseName));
    }

    protected function success(mixed $payload = null): WorkflowToolResult
    {
        return WorkflowToolResult::success($payload);
    }

    protected function fail(string $reason, mixed $details = null): WorkflowToolResult
    {
        return WorkflowToolResult::fail($reason, $details);
    }
}
