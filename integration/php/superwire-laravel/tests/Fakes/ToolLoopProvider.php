<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Fakes;

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

        $prompt = $request->prompt();

        if ($prompt === null || !array_key_exists($prompt, $this->resultsByPrompt)) {
            throw new RuntimeException(sprintf('No fake tool-loop response registered for prompt: %s', $prompt ?? 'null'));
        }

        $result = $this->resultsByPrompt[ $prompt ];

        if ($result instanceof Throwable) {
            throw $result;
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
