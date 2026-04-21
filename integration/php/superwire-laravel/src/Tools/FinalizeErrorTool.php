<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Tool;

final class FinalizeErrorTool implements WorkflowTool
{
    private bool $wasCalled = false;

    private ?string $reason = null;

    public function name(): string
    {
        return 'finalize_error';
    }

    public function toPrismTool(array $boundArguments = []): Tool
    {
        $tool = new Tool();

        return $tool
            ->as($this->name())
            ->for('Finish the agent with an error message when the task cannot be completed.')
            ->withoutErrorHandling()
            ->withStringParameter('message', 'The reason the agent cannot complete successfully.')
            ->using(function (string $message): string {
                $this->wasCalled = true;
                $this->reason = $message;

                return 'OK';
            });
    }

    public function wasCalled(): bool
    {
        return $this->wasCalled;
    }

    public function reason(): ?string
    {
        return $this->reason;
    }

    public function reset(): void
    {
        $this->wasCalled = false;
        $this->reason = null;
    }
}
