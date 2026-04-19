<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

use Swaggest\JsonSchema\Schema;

interface RuntimeToolSchemaProviderInterface
{
    public function schemaForTool(string $toolName): ?Schema;
}
