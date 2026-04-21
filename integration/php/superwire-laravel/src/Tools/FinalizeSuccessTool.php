<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools;

use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Tool;
use Prism\Prism\ValueObjects\ToolOutput;

final class FinalizeSuccessTool extends Tool
{
    private mixed $result = null;

    private bool $wasCalled = false;

    /**
     * @param array<string, mixed> $outputSchema
     */
    public function __construct(array $outputSchema)
    {
        parent::__construct();

        $this
            ->as('finalize_success')
            ->for('Finish the agent successfully with the final output payload.')
            ->withParameter(new RawSchema('result', $outputSchema))
            ->using(function (mixed $result): ToolOutput {
                $this->wasCalled = true;
                $this->result = $result;

                return new ToolOutput('success finalized');
            });
    }

    public function wasCalled(): bool
    {
        return $this->wasCalled;
    }

    public function result(): mixed
    {
        return $this->result;
    }
}
