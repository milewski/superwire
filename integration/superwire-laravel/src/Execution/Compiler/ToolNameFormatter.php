<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Execution\Compiler;

final class ToolNameFormatter
{
    public function moduleName(string $toolName): string
    {
        return str_replace('-', '_', $toolName);
    }

    public function typeName(string $toolName): string
    {
        $segments = preg_split('/[_\-]+/', $toolName) ?: [ $toolName ];
        $typeName = '';

        foreach ($segments as $segment) {

            if ($segment === '') {
                continue;
            }

            $typeName .= ucfirst($segment);

        }

        return $typeName === '' ? 'ProxyTool' : $typeName;
    }
}
