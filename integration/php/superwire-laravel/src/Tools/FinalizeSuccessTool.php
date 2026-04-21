<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Tool;
use Superwire\Laravel\Exceptions\FinalizeSuccess;

final class FinalizeSuccessTool extends Tool
{
    /**
     * @param array<string, mixed> $outputSchema
     */
    public function __construct(array $outputSchema)
    {
        parent::__construct();

        $this
            ->as('finalize_success')
            ->for('Finish the agent successfully with the final output payload.')
            ->withoutErrorHandling()
            ->withParameter(new RawSchema('result', $outputSchema))
            ->using(fn (mixed $result) => throw new FinalizeSuccess($result));
    }
}
