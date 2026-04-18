<?php

declare(strict_types=1);

namespace Superwire\Contracts\Support;

final class ExecutionPipeline
{
    /**
     * @var array<int, callable(mixed): mixed>
     */
    private array $stages = [];

    public function addStage(callable $stage): self
    {
        $this->stages[] = $stage;

        return $this;
    }

    public function run(mixed $context): mixed
    {
        $currentContext = $context;

        foreach ($this->stages as $stage) {
            $currentContext = $stage($currentContext);
        }

        return $currentContext;
    }
}
