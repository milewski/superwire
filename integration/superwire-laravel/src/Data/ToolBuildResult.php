<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data;

final readonly class ToolBuildResult
{
    /**
     * @param list<string> $toolNames
     */
    public function __construct(
        public array $toolNames,
        public string $outputDirectory,
    )
    {
    }
}
