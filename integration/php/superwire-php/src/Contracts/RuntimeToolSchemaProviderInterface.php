<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

interface RuntimeToolSchemaProviderInterface
{
    /**
     * @return array<string, mixed>|null
     */
    public function schemaForTool(string $toolName): ?array;
}
