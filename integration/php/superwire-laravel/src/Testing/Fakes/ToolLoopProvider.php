<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Testing\Fakes;

use Generator;
use Prism\Prism\Streaming\EventID;
use Prism\Prism\Streaming\Events\StepFinishEvent;
use Prism\Prism\Streaming\Events\StepStartEvent;
use Prism\Prism\Streaming\Events\StreamEndEvent;
use Prism\Prism\Streaming\Events\StreamEvent;
use Prism\Prism\Streaming\Events\StreamStartEvent;
use Prism\Prism\Streaming\Events\ToolCallEvent;
use Prism\Prism\Streaming\Events\ToolResultEvent;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use RuntimeException;
use Throwable;

final class ToolLoopProvider extends AbstractToolLoopProvider
{
    /**
     * @param array<string, mixed> $resultsByPrompt
     */
    public function __construct(
        private readonly array $resultsByPrompt,
    )
    {
    }

    public function text(TextRequest $request): TextResponseFake
    {
        $this->recordTextRequest($request);

        return $this->responseForRequest($request);
    }

    /**
     * @return Generator<StreamEvent>
     */
    public function stream(TextRequest $request): Generator
    {
        $this->recordStreamRequest($request);

        $response = $this->responseForRequest($request);
        $messageId = EventID::generate();

        yield new StreamStartEvent(
            id: EventID::generate(),
            timestamp: time(),
            model: $request->model(),
            provider: 'fake',
        );

        yield new StepStartEvent(
            id: EventID::generate(),
            timestamp: time(),
        );

        foreach ($response->toolCalls as $toolCall) {

            yield new ToolCallEvent(
                id: EventID::generate(),
                timestamp: time(),
                toolCall: $toolCall,
                messageId: $messageId,
            );

        }

        foreach ($response->toolResults as $toolResult) {

            yield new ToolResultEvent(
                id: EventID::generate(),
                timestamp: time(),
                toolResult: $toolResult,
                messageId: $messageId,
                success: true,
            );

        }

        yield new StepFinishEvent(
            id: EventID::generate(),
            timestamp: time(),
        );

        yield new StreamEndEvent(
            id: EventID::generate(),
            timestamp: time(),
            finishReason: $response->finishReason,
            usage: $response->usage,
        );
    }

    private function responseForRequest(TextRequest $request): TextResponseFake
    {
        $result = $this->resultForPrompt($request->prompt());

        if ($result instanceof FinalizeErrorResponse) {
            return $this->finalizeErrorResponse($request, $result->message);
        }

        if ($result instanceof NoFinalizationResponse) {
            return $this->textResponse($request, $result->text);
        }

        return $this->finalizeSuccessResponse($request, $result);
    }

    private function resultForPrompt(?string $prompt): mixed
    {
        if ($prompt === null || !array_key_exists($prompt, $this->resultsByPrompt)) {
            throw new RuntimeException(sprintf('No fake tool-loop response registered for prompt: %s', $prompt ?? 'null'));
        }

        $result = $this->resultsByPrompt[ $prompt ];

        if ($result instanceof Throwable) {
            throw $result;
        }

        return $result;
    }
}
