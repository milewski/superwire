<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Testing\Fakes;

use Illuminate\Support\Collection;
use Prism\Prism\Enums\FinishReason;
use Prism\Prism\Providers\Provider;
use Prism\Prism\Testing\TextResponseFake;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\Text\Step;
use Prism\Prism\Tool;
use Prism\Prism\ValueObjects\Messages\AssistantMessage;
use Prism\Prism\ValueObjects\Messages\ToolResultMessage;
use Prism\Prism\ValueObjects\Meta;
use Prism\Prism\ValueObjects\ToolCall;
use Prism\Prism\ValueObjects\ToolError;
use Prism\Prism\ValueObjects\ToolResult;
use Prism\Prism\ValueObjects\Usage;
use RuntimeException;
use Superwire\Laravel\Testing\ToolCallFactory;
use Superwire\Laravel\Tools\Internal\FinalizeErrorTool;
use Superwire\Laravel\Tools\Internal\FinalizeSuccessTool;
use Superwire\Laravel\Tools\WorkflowTool;

abstract class AbstractToolLoopProvider extends Provider
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
     * @var array<int, ToolCall>
     */
    private array $toolCalls = [];

    /**
     * @var array<int, ToolResult>
     */
    private array $toolResults = [];

    protected function recordTextRequest(TextRequest $request): void
    {
        $this->requests[] = $request;
        $this->textRequests[] = $request;
    }

    protected function recordStreamRequest(TextRequest $request): void
    {
        $this->requests[] = $request;
        $this->streamRequests[] = $request;
    }

    public function recordProviderConfig(array $providerConfig): void
    {
        $this->providerConfigs[] = $providerConfig;
    }

    public function request(int $index): ?TextRequest
    {
        return $this->requests[ $index ] ?? null;
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

    public function toolCall(int $index): ?ToolCall
    {
        return $this->toolCalls[ $index ] ?? null;
    }

    /**
     * @return array<int, ToolCall>
     */
    public function toolCalls(): array
    {
        return $this->toolCalls;
    }

    public function toolResult(int $index): ?ToolResult
    {
        return $this->toolResults[ $index ] ?? null;
    }

    /**
     * @return array<int, ToolResult>
     */
    public function toolResults(): array
    {
        return $this->toolResults;
    }

    public function registeredTool(string $toolName, int $requestIndex = 0): ?Tool
    {
        $request = $this->request($requestIndex);

        if (!$request instanceof TextRequest) {
            return null;
        }

        foreach ($request->tools() as $tool) {

            if ($tool->name() === $toolName) {
                return $tool;
            }

        }

        return null;
    }

    public function executeToolCall(TextRequest $request, ToolCall $toolCall): ToolResult
    {
        $tool = $this->resolveTool($toolCall->name, $request->tools());
        $output = $tool->handle(...$toolCall->arguments());

        $toolResult = new ToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            args: $toolCall->arguments(),
            result: $output instanceof ToolError ? $output->message : $output,
        );

        $this->toolCalls[] = $toolCall;
        $this->toolResults[] = $toolResult;

        return $toolResult;
    }

    public function callTool(TextRequest $request, ToolCall $toolCall): TextResponseFake
    {
        return $this->toolResponse($request, $toolCall, $this->executeToolCall($request, $toolCall));
    }

    /**
     * @param class-string<WorkflowTool>|WorkflowTool $tool
     * @param array<string, mixed> $arguments
     */
    public function callToolClass(TextRequest $request, string|WorkflowTool $tool, array $arguments = [], ?string $toolCallId = null): TextResponseFake
    {
        return $this->callTool($request, ToolCallFactory::fromClass($tool, $arguments, $toolCallId));
    }

    public function finalizeSuccessResponse(TextRequest $request, mixed $result, string $toolCallId = 'fake-finalize-success'): TextResponseFake
    {
        return $this->callTool(
            request: $request,
            toolCall: ToolCallFactory::fromClass(FinalizeSuccessTool::class, [ 'result' => $result ], $toolCallId),
        );
    }

    public function finalizeErrorResponse(TextRequest $request, string $message, string $toolCallId = 'fake-finalize-error'): TextResponseFake
    {
        return $this->callTool(
            request: $request,
            toolCall: ToolCallFactory::fromClass(FinalizeErrorTool::class, [ 'message' => $message ], $toolCallId),
        );
    }

    public function toolResponse(TextRequest $request, ToolCall $toolCall, ToolResult $toolResult): TextResponseFake
    {
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

    public function textResponse(TextRequest $request, string $text): TextResponseFake
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

    /**
     * @param array<int, Tool> $tools
     */
    protected function resolveTool(string $name, array $tools): Tool
    {
        foreach ($tools as $tool) {

            if ($tool->name() === $name) {
                return $tool;
            }

        }

        throw new RuntimeException(sprintf('Tool not found in fake provider: %s', $name));
    }
}
