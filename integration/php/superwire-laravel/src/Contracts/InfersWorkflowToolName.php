<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Contracts;

trait InfersWorkflowToolName
{
    public static function toolName(): string
    {
        $className = static::class;
        $classBaseName = $className;

        if (str_contains($className, '\\')) {

            $segments = explode('\\', $className);
            $classBaseName = (string) end($segments);

        }

        $snakeName = strtolower((string) preg_replace('/(?<!^)[A-Z]/', '_$0', $classBaseName));

        return $snakeName;
    }
}
