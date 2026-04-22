<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tools\Internal;

use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Tool;
use Superwire\Laravel\Tools\WorkflowTool;

class FinalizeSuccessTool implements WorkflowTool
{
    private bool $wasCalled = false;
    private mixed $result = null;

    /**
     * @param array<string, mixed> $outputSchema
     */
    public function __construct(
        private readonly array $outputSchema,
    )
    {
    }

    public static function name(): string
    {
        return 'finalize_success';
    }

    public function toPrismTool(array $boundArguments = []): Tool
    {
        $tool = new Tool();

        return $tool
            ->as(static::name())
            ->for('Finish the agent successfully with the final output payload.')
            ->withoutErrorHandling()
            ->withParameter(new RawSchema('result', $this->outputSchema))
            ->using(function (mixed $result): string {

                $this->wasCalled = true;
                $this->result = $result;

                return 'OK';

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

    public function reset(): void
    {
        $this->wasCalled = false;
        $this->result = null;
    }
}
