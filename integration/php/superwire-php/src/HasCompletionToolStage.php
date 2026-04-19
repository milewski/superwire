<?php

declare(strict_types = 1);

namespace Superwire\Contracts;

use Superwire\Contracts\Support\Stages\CompletionToolLoopStage;

trait HasCompletionToolStage
{
    protected CompletionToolLoopStage $completionStage;

    protected function setUp(): void
    {
        parent::setUp();

        $this->completionStage = new CompletionToolLoopStage();
    }
}
