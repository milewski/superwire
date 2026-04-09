<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Data;

final readonly class ToolBuildRequest
{
    /**
     * @param list<class-string> $toolClasses
     */
    public function __construct(public array $toolClasses)
    {
    }
}
