<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Fakes;

use Illuminate\Support\Collection;
use Prism\Prism\Enums\FinishReason;
use Prism\Prism\Providers\Provider;
use Prism\Prism\Text\Request as TextRequest;
use Prism\Prism\Text\Response as TextResponse;
use Prism\Prism\Text\Step;
use Prism\Prism\Tool;
use Prism\Prism\ValueObjects\Messages\AssistantMessage;
use Prism\Prism\ValueObjects\Messages\ToolResultMessage;
use Prism\Prism\ValueObjects\Meta;
use Prism\Prism\ValueObjects\ToolCall;
use Prism\Prism\ValueObjects\ToolResult;
use Prism\Prism\ValueObjects\Usage;
use RuntimeException;

final class ToolLoopProvider extends Provider
{
    /**
     * @param array<string, mixed> $resultsByPrompt
     */
    public function __construct(
        private readonly array $resultsByPrompt,
    ) {
    }

    public function text(TextRequest $request): TextResponse
    {
        $prompt = $request->prompt();

        if ($prompt === null || ! array_key_exists($prompt, $this->resultsByPrompt)) {
            throw new RuntimeException(sprintf('No fake tool-loop response registered for prompt: %s', $prompt ?? 'null'));
        }

        $result = $this->resultsByPrompt[$prompt];
        $finalizeTool = $this->resolveTool('finalize_success', $request->tools());
        $toolCall = new ToolCall(
            id: uniqid('tool_call_', true),
            name: 'finalize_success',
            arguments: ['result' => $result],
        );

        $toolOutput = $finalizeTool->handle(...['result' => $result]);
        $toolResult = new ToolResult(
            toolCallId: $toolCall->id,
            toolName: $toolCall->name,
            args: $toolCall->arguments(),
            result: is_string($toolOutput) ? $toolOutput : $toolOutput->result,
        );

        $assistantMessage = new AssistantMessage('', [$toolCall]);
        $toolResultMessage = new ToolResultMessage([$toolResult]);
        $messages = collect([...$request->messages(), $assistantMessage, $toolResultMessage]);
        $step = new Step(
            text: '',
            finishReason: FinishReason::ToolCalls,
            toolCalls: [$toolCall],
            toolResults: [$toolResult],
            providerToolCalls: [],
            usage: new Usage(0, 0),
            meta: new Meta('tool-loop-fake', $request->model()),
            messages: $request->messages(),
            systemPrompts: $request->systemPrompts(),
        );

        return new TextResponse(
            steps: new Collection([$step]),
            text: '',
            finishReason: FinishReason::ToolCalls,
            toolCalls: [$toolCall],
            toolResults: [$toolResult],
            usage: new Usage(0, 0),
            meta: new Meta('tool-loop-fake', $request->model()),
            messages: $messages,
            additionalContent: [],
        );
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
