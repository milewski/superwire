<?php

declare(strict_types=1);

namespace Superwire\Laravel\Data;

use Spatie\LaravelData\Data;
use Superwire\Contracts\ToolExecution;

final class ResolvedToolData extends Data
{
    /**
     * @param array<string, mixed> $bindings
     */
    public function __construct(
        public string $name,
        public array $bindings,
    ) {
    }

    public function toToolExecution(): ToolExecution
    {
        return new ToolExecution($this->name, $this->bindings);
    }
}
