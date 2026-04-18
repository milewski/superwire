<?php

declare(strict_types=1);

namespace Superwire\Laravel\Driver;

use Prism\Prism\Enums\Provider as PrismProvider;
use Prism\Prism\Enums\ToolChoice;
use Prism\Prism\Facades\Prism;
use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Text\Response as PrismTextResponse;
use Prism\Prism\Tool as PrismTool;
use Prism\Prism\ValueObjects\Messages\UserMessage;
use Prism\Prism\ValueObjects\ToolCall;
use RuntimeException;
use Superwire\Contracts\AgentConversationMessage;
use Superwire\Contracts\AgentToolCall;
use Superwire\Contracts\AgentToolDefinition;
use Superwire\Contracts\AgentToolResult;
use Superwire\Contracts\AgentTurnRequest;
use Superwire\Contracts\AgentTurnResponse;
use Superwire\Contracts\Contracts\AgentTurnDriverInterface;
use Throwable;

final class PrismAgentDriver implements AgentTurnDriverInterface
{
    public function generateTurn(AgentTurnRequest $request): AgentTurnResponse
    {
        $provider = $this->resolvePrismProvider($request->provider);
        $providerConfig = $this->normalizeProviderConfig($request->providerConfig);
        $messages = $this->toPrismMessages($request->messages);
        $tools = $this->toPrismTools($request->tools);

        try {
            $response = Prism::text()
                ->using($provider, $request->model, $providerConfig)
                ->withMessages($messages)
                ->withTools($tools)
                ->withToolChoice($request->requireToolCall ? ToolChoice::Any : ToolChoice::Auto)
                ->withMaxSteps(1)
                ->asText();
        } catch (Throwable $throwable) {
            if ($this->canUseOpenAiFallback($provider, $providerConfig)) {
                return $this->requestOpenAiCompatibleTurn($request, $providerConfig);
            }

            throw $throwable;
        }

        return new AgentTurnResponse(
            text: $response->text,
            toolCalls: $this->toToolCalls($response->toolCalls),
            toolResults: $this->toToolResults($response->toolResults),
        );
    }

    /**
     * @param array<string, mixed> $providerConfig
     */
    private function canUseOpenAiFallback(PrismProvider $provider, array $providerConfig): bool
    {
        if ($provider !== PrismProvider::OpenAI) {
            return false;
        }

        return is_string($providerConfig['url'] ?? null) && is_string($providerConfig['api_key'] ?? null);
    }

    /**
     * @param array<string, mixed> $providerConfig
     */
    private function requestOpenAiCompatibleTurn(AgentTurnRequest $request, array $providerConfig): AgentTurnResponse
    {
        $url = rtrim((string) $providerConfig['url'], '/') . '/chat/completions';
        $apiKey = (string) $providerConfig['api_key'];
        $payload = [
            'model' => $request->model,
            'messages' => [
                [
                    'role' => 'user',
                    'content' => $this->conversationToPrompt($request->messages),
                ],
            ],
            'tools' => array_map(
                static fn (AgentToolDefinition $tool): array => [
                    'type' => 'function',
                    'function' => [
                        'name' => $tool->name,
                        'description' => $tool->description,
                        'parameters' => $tool->parametersSchema,
                    ],
                ],
                $request->tools,
            ),
            'tool_choice' => $request->requireToolCall ? 'required' : 'auto',
        ];

        $httpContext = stream_context_create([
            'http' => [
                'method' => 'POST',
                'header' => implode("\r\n", [
                    'Content-Type: application/json',
                    'Authorization: Bearer ' . $apiKey,
                ]),
                'content' => json_encode($payload, JSON_UNESCAPED_SLASHES),
                'ignore_errors' => true,
            ],
        ]);

        $responseBody = @file_get_contents($url, false, $httpContext);

        if (!is_string($responseBody)) {
            throw new RuntimeException('openai-compatible fallback request failed');
        }

        $decodedResponse = json_decode($responseBody, true);

        if (!is_array($decodedResponse)) {
            throw new RuntimeException('openai-compatible fallback returned invalid json');
        }

        $message = $decodedResponse['choices'][0]['message'] ?? null;

        if (is_array($message)) {
            $toolCalls = [];

            foreach (($message['tool_calls'] ?? []) as $toolCall) {
                if (!is_array($toolCall) || !is_array($toolCall['function'] ?? null)) {
                    continue;
                }

                $arguments = json_decode((string) ($toolCall['function']['arguments'] ?? '{}'), true);

                if (!is_array($arguments)) {
                    $arguments = [];
                }

                $toolCalls[] = new AgentToolCall(
                    id: (string) ($toolCall['id'] ?? uniqid('tool-call-', true)),
                    name: (string) ($toolCall['function']['name'] ?? ''),
                    arguments: $arguments,
                );
            }

            return new AgentTurnResponse(
                text: (string) ($message['content'] ?? ''),
                toolCalls: $toolCalls,
                toolResults: [],
            );
        }

        $outputText = $decodedResponse['output_text'] ?? null;

        if (is_string($outputText)) {
            return new AgentTurnResponse(
                text: $outputText,
                toolCalls: [],
                toolResults: [],
            );
        }

        throw new RuntimeException('openai-compatible fallback response missing first choice message: ' . $responseBody);
    }

    /**
     * @param array<string, mixed> $providerConfig
     * @return array<string, mixed>
     */
    private function normalizeProviderConfig(array $providerConfig): array
    {
        if (array_is_list($providerConfig)) {
            $normalized = [];

            foreach ($providerConfig as $value) {
                if (!is_string($value)) {
                    continue;
                }

                if (str_starts_with($value, 'http://') || str_starts_with($value, 'https://')) {
                    $normalized['url'] = $value;

                    continue;
                }

                if (str_starts_with($value, 'sk-')) {
                    $normalized['api_key'] = $value;

                    continue;
                }

                if (!array_key_exists('provider', $normalized)) {
                    $normalized['provider'] = $value;
                }
            }

            $providerConfig = $normalized;
        }

        if (array_key_exists('endpoint', $providerConfig) && !array_key_exists('url', $providerConfig)) {
            $providerConfig['url'] = $providerConfig['endpoint'];
            unset($providerConfig['endpoint']);
        }

        return $providerConfig;
    }

    /** @param array<int, AgentConversationMessage> $messages */
    private function toPrismMessages(array $messages): array
    {
        return [new UserMessage($this->conversationToPrompt($messages))];
    }

    /** @param array<int, AgentConversationMessage> $messages */
    private function conversationToPrompt(array $messages): string
    {
        $segments = [];

        foreach ($messages as $message) {
            if ($message->role === 'user') {
                $segments[] = "[user]\n" . (string) ($message->payload['content'] ?? '');

                continue;
            }

            if ($message->role === 'assistant') {
                $content = (string) ($message->payload['content'] ?? '');
                $segments[] = "[assistant]\n{$content}";

                $toolCalls = $message->payload['tool_calls'] ?? [];

                foreach ($toolCalls as $toolCall) {
                    if (!$toolCall instanceof AgentToolCall) {
                        continue;
                    }

                    $segments[] = "[assistant_tool_call]\n" . json_encode([
                        'id' => $toolCall->id,
                        'name' => $toolCall->name,
                        'arguments' => $toolCall->arguments,
                    ], JSON_UNESCAPED_SLASHES);
                }

                continue;
            }

            if ($message->role === 'tool_result') {
                $toolResults = $message->payload['tool_results'] ?? [];

                foreach ($toolResults as $toolResult) {
                    if (!$toolResult instanceof AgentToolResult) {
                        continue;
                    }

                    $segments[] = "[tool_result]\n" . json_encode([
                        'tool_call_id' => $toolResult->toolCallId,
                        'tool_name' => $toolResult->toolName,
                        'arguments' => $toolResult->arguments,
                        'result' => $toolResult->result,
                    ], JSON_UNESCAPED_SLASHES);
                }
            }
        }

        return implode("\n\n", $segments);
    }

    /**
     * @param array<int, AgentToolDefinition> $tools
     * @return array<int, PrismTool>
     */
    private function toPrismTools(array $tools): array
    {
        $prismTools = [];

        foreach ($tools as $tool) {
            $prismTool = (new PrismTool())
                ->as($tool->name)
                ->for($tool->description)
                ->using(static fn (): string => 'ok');

            $properties = $tool->parametersSchema['properties'] ?? null;

            if (is_array($properties)) {
                foreach ($properties as $parameterName => $parameterSchema) {
                    if (!is_string($parameterName) || !is_array($parameterSchema)) {
                        continue;
                    }

                    $prismTool = $prismTool->withParameter(new RawSchema($parameterName, $parameterSchema));
                }
            }

            $prismTools[] = $prismTool;
        }

        return $prismTools;
    }

    /**
     * @param array<int, ToolCall> $toolCalls
     * @return array<int, AgentToolCall>
     */
    private function toToolCalls(array $toolCalls): array
    {
        $normalizedToolCalls = [];

        foreach ($toolCalls as $toolCall) {
            try {
                $arguments = $toolCall->arguments();
            } catch (Throwable) {
                $arguments = [];
            }

            $normalizedToolCalls[] = new AgentToolCall($toolCall->id, $toolCall->name, $arguments);
        }

        return $normalizedToolCalls;
    }

    /** @return array<int, AgentToolResult> */
    private function toToolResults(array $toolResults): array
    {
        return [];
    }

    private function resolvePrismProvider(string $providerIdentifier): PrismProvider
    {
        $normalizedIdentifier = strtolower(trim($providerIdentifier));
        $normalizedIdentifier = str_replace(['-', ' '], '', $normalizedIdentifier);
        $providerMap = [
            'openai' => PrismProvider::OpenAI,
            'anthropic' => PrismProvider::Anthropic,
            'ollama' => PrismProvider::Ollama,
            'openrouter' => PrismProvider::OpenRouter,
            'deepseek' => PrismProvider::DeepSeek,
            'groq' => PrismProvider::Groq,
            'mistral' => PrismProvider::Mistral,
            'gemini' => PrismProvider::Gemini,
            'xai' => PrismProvider::XAI,
            'voyageai' => PrismProvider::VoyageAI,
            'elevenlabs' => PrismProvider::ElevenLabs,
            'perplexity' => PrismProvider::Perplexity,
            'z' => PrismProvider::Z,
        ];

        $provider = $providerMap[$normalizedIdentifier] ?? null;

        if ($provider !== null) {
            return $provider;
        }

        throw new RuntimeException("unsupported prism provider `{$providerIdentifier}`");
    }
}
