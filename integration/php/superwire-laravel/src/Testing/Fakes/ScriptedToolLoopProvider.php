<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Testing\Fakes;

use Generator;
use Prism\Prism\Streaming\Events\StreamEvent;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use RuntimeException;

class ScriptedToolLoopProvider extends AbstractToolLoopProvider
{
    /**
     * @var array<int, callable(TextRequest, self): TextResponseFake>
     */
    private array $steps;

    /**
     * @param array<int, callable(TextRequest, self): TextResponseFake> $steps
     */
    public function __construct(array $steps)
    {
        $this->steps = array_values($steps);
    }

    public function text(TextRequest $request): TextResponseFake
    {
        $this->recordTextRequest($request);
        $stepIndex = count($this->requests()) - 1;
        $step = $this->steps[ $stepIndex ] ?? null;

        if (!is_callable($step)) {
            throw new RuntimeException(sprintf('No scripted tool-loop step registered for request #%d.', $stepIndex + 1));
        }

        return $step($request, $this);
    }

    /**
     * @return Generator<StreamEvent>
     */
    public function stream(TextRequest $request): Generator
    {
        throw new RuntimeException('Streaming is not supported by ScriptedToolLoopProvider.');
    }
}
