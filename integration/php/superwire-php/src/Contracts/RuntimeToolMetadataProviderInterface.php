<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Contracts;

interface RuntimeToolMetadataProviderInterface
{
    public function descriptionForTool(string $toolName): ?string;

    public function strictSchemaForTool(string $toolName): ?bool;
}
