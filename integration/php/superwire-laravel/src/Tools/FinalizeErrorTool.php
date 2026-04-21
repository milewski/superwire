<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Tool;
use Superwire\Laravel\Exceptions\FinalizeError;

final class FinalizeErrorTool extends Tool
{
    public function __construct()
    {
        parent::__construct();

        $this
            ->as('finalize_error')
            ->for('Finish the agent with an error message when the task cannot be completed.')
            ->withoutErrorHandling()
            ->withStringParameter('message', 'The reason the agent cannot complete successfully.')
            ->using(fn (string $message) => throw new FinalizeError($message));
    }
}
