<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Fakes;

use Generator;
use Illuminate\Support\Collection;
use Prism\Prism\Enums\FinishReason;
use Prism\Prism\Providers\Provider;
use Prism\Prism\Streaming\EventID;
use Prism\Prism\Streaming\Events\StepFinishEvent;
use Prism\Prism\Streaming\Events\StepStartEvent;
use Prism\Prism\Streaming\Events\StreamEndEvent;
use Prism\Prism\Streaming\Events\StreamStartEvent;
use Prism\Prism\Streaming\Events\ToolCallEvent;
use Prism\Prism\Streaming\Events\ToolResultEvent;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\Text\Step;
use Prism\Prism\Tool;
use Prism\Prism\ValueObjects\Messages\AssistantMessage;
use Prism\Prism\ValueObjects\Messages\ToolResultMessage;
use Prism\Prism\ValueObjects\Meta;
use Prism\Prism\ValueObjects\ToolCall;
use Prism\Prism\ValueObjects\ToolResult;
use Prism\Prism\ValueObjects\Usage;
use RuntimeException;
use Throwable;

final class ToolLoopProvider extends Provider
{
    /**
     * @var array<int, TextRequest>
     */
    private array $requests = [];

    /**
     * @var array<int, TextRequest>
     */
    private array $textRequests = [];

    /**
     * @var array<int, TextRequest>
     */
    private array $streamRequests = [];

    /**
     * @var array<int, array<string, mixed>>
     */
    private array $providerConfigs = [];

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
        $this->requests[] = $request;
        $this->textRequests[] = $request;

        return $this->responseForRequest($request);
    }

    /**
     * @return Generator<\Prism\Prism\Streaming\Events\StreamEvent>
     */
    public function stream(TextRequest $request): Generator
    {
        $this->requests[] = $request;
        $this->streamRequests[] = $request;

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
            return $this->unfinishedResponse($request, $result->text);
        }

        $finalizeTool = $this->resolveTool('finalize_success', $request->tools());

        $toolCall = new ToolCall(
            id: 'fake-finalize-success',
            name: 'finalize_success',
            arguments: [ 'result' => $result ],
        );

        $toolResultValue = $finalizeTool->handle(...[ 'result' => $result ]);

        $toolResult = new ToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            args: $toolCall->arguments(),
            result: $toolResultValue,
        );

        $assistantMessage = new AssistantMessage(content: '', toolCalls: [ $toolCall ]);
        $toolResultMessage = new ToolResultMessage([ $toolResult ]);

        return TextResponseFake::make()
            ->withFinishReason(FinishReason::ToolCalls)
            ->withToolCalls([ $toolCall ])
            ->withToolResults([ $toolResult ])
            ->withUsage(new Usage(0, 0))
            ->withMeta(new Meta('fake', 'fake'))
            ->withSteps(collect([
                new Step(
                    text: '',
                    finishReason: FinishReason::ToolCalls,
                    toolCalls: [ $toolCall ],
                    toolResults: [ $toolResult ],
                    providerToolCalls: [],
                    usage: new Usage(0, 0),
                    meta: new Meta('fake', 'fake'),
                    messages: $request->messages(),
                    systemPrompts: $request->systemPrompts(),
                ),
            ]))
            ->withMessages(new Collection([
                ...$request->messages(),
                $assistantMessage,
                $toolResultMessage,
            ]));
    }

    private function finalizeErrorResponse(TextRequest $request, string $message): TextResponseFake
    {
        $finalizeTool = $this->resolveTool('finalize_error', $request->tools());

        $toolCall = new ToolCall(
            id: 'fake-finalize-error',
            name: 'finalize_error',
            arguments: [ 'message' => $message ],
        );

        $toolResultValue = $finalizeTool->handle(...[ 'message' => $message ]);

        $toolResult = new ToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            args: $toolCall->arguments(),
            result: $toolResultValue,
        );

        $assistantMessage = new AssistantMessage(content: '', toolCalls: [ $toolCall ]);
        $toolResultMessage = new ToolResultMessage([ $toolResult ]);

        return TextResponseFake::make()
            ->withFinishReason(FinishReason::ToolCalls)
            ->withToolCalls([ $toolCall ])
            ->withToolResults([ $toolResult ])
            ->withUsage(new Usage(0, 0))
            ->withMeta(new Meta('fake', 'fake'))
            ->withSteps(collect([
                new Step(
                    text: '',
                    finishReason: FinishReason::ToolCalls,
                    toolCalls: [ $toolCall ],
                    toolResults: [ $toolResult ],
                    providerToolCalls: [],
                    usage: new Usage(0, 0),
                    meta: new Meta('fake', 'fake'),
                    messages: $request->messages(),
                    systemPrompts: $request->systemPrompts(),
                ),
            ]))
            ->withMessages(new Collection([
                ...$request->messages(),
                $assistantMessage,
                $toolResultMessage,
            ]));
    }

    private function unfinishedResponse(TextRequest $request, string $text): TextResponseFake
    {
        $assistantMessage = new AssistantMessage(content: $text, toolCalls: []);

        return TextResponseFake::make()
            ->withText($text)
            ->withFinishReason(FinishReason::Stop)
            ->withToolCalls([])
            ->withToolResults([])
            ->withUsage(new Usage(0, 0))
            ->withMeta(new Meta('fake', 'fake'))
            ->withSteps(collect([
                new Step(
                    text: $text,
                    finishReason: FinishReason::Stop,
                    toolCalls: [],
                    toolResults: [],
                    providerToolCalls: [],
                    usage: new Usage(0, 0),
                    meta: new Meta('fake', 'fake'),
                    messages: $request->messages(),
                    systemPrompts: $request->systemPrompts(),
                ),
            ]))
            ->withMessages(new Collection([
                ...$request->messages(),
                $assistantMessage,
            ]));
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

    /**
     * @param array<string, mixed> $providerConfig
     */
    public function recordProviderConfig(array $providerConfig): void
    {
        $this->providerConfigs[] = $providerConfig;
    }

    /**
     * @return array<int, TextRequest>
     */
    public function requests(): array
    {
        return $this->requests;
    }

    /**
     * @return array<int, TextRequest>
     */
    public function textRequests(): array
    {
        return $this->textRequests;
    }

    /**
     * @return array<int, TextRequest>
     */
    public function streamRequests(): array
    {
        return $this->streamRequests;
    }

    /**
     * @return array<int, array<string, mixed>>
     */
    public function providerConfigs(): array
    {
        return $this->providerConfigs;
    }

    /**
     * @param array<int, Tool> $tools
     */
    private function resolveTool(string $name, array $tools): Tool
    {
        foreach ($tools as $tool) {
            if ($tool->name() === $name) {
                return $tool;
            }
        }

        throw new RuntimeException(sprintf('Tool not found in fake provider: %s', $name));
    }
}
