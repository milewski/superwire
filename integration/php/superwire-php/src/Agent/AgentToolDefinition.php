<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Agent;

use Swaggest\JsonSchema\Schema;

final class AgentToolDefinition
{
    public function __construct(
        public readonly string $name,
        public readonly string $description,
        public readonly Schema $parametersSchema,
        public readonly bool $strict = true,
    )
    {
    }
}
