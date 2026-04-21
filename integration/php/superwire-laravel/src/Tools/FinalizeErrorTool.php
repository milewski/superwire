<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Tool;
use Prism\Prism\ValueObjects\ToolOutput;

final class FinalizeErrorTool extends Tool
{
    private ?string $message = null;

    private bool $wasCalled = false;

    public function __construct()
    {
        parent::__construct();

        $this
            ->as('finalize_error')
            ->for('Finish the agent with an error message when the task cannot be completed.')
            ->withStringParameter('message', 'The reason the agent cannot complete successfully.')
            ->using(function (string $message): ToolOutput {
                $this->wasCalled = true;
                $this->message = $message;

                return new ToolOutput('error finalized');
            });
    }

    public function wasCalled(): bool
    {
        return $this->wasCalled;
    }

    public function message(): ?string
    {
        return $this->message;
    }
}
