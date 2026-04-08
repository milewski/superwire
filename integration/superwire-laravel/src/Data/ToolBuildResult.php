<?php

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
