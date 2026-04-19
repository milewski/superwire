<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Driver;

use Illuminate\Support\Collection;
use JsonException;
use Prism\Prism\Enums\Provider as PrismProvider;
use Prism\Prism\Enums\ToolChoice;
use Prism\Prism\Facades\Prism;
use Prism\Prism\Schema\RawSchema;
use Prism\Prism\Streaming\Events\TextDeltaEvent;
use Prism\Prism\Streaming\Events\ToolCallEvent;
use Prism\Prism\Streaming\Events\ToolResultEvent;
use Prism\Prism\Text\PendingRequest as PrismPendingTextRequest;
use Prism\Prism\Tool as PrismTool;
use Prism\Prism\ValueObjects\Messages\AssistantMessage;
use Prism\Prism\ValueObjects\Messages\ToolResultMessage;
use Prism\Prism\ValueObjects\Messages\UserMessage;
use Prism\Prism\ValueObjects\ToolCall;
use Prism\Prism\ValueObjects\ToolResult as PrismToolResult;
use RuntimeException;
use Superwire\Contracts\Agent\AgentConversationMessage;
use Superwire\Contracts\Agent\AgentToolCall;
use Superwire\Contracts\Agent\AgentToolDefinition;
use Superwire\Contracts\Agent\AgentToolResult;
use Superwire\Contracts\Agent\AgentTurnRequest;
use Superwire\Contracts\Agent\AgentTurnResponse;
use Superwire\Contracts\Agent\ConversationRole;
use Superwire\Contracts\Contracts\AgentTurnDriverInterface;
use Swaggest\JsonSchema\Schema;
use Throwable;

final readonly class PrismAgentDriver implements AgentTurnDriverInterface
{
    /**
     * @param array<string, mixed> $driverConfiguration
     */
    public function __construct(
        private array $driverConfiguration = [],
    )
    {
    }

    public function generateTurn(AgentTurnRequest $request): AgentTurnResponse
    {
        $providerConfig = $this->normalizeProviderConfig($request->providerConfig);
        $provider = $this->resolvePrismProvider($request->provider);
        $messages = $this->toPrismMessages($request->messages);
        $tools = $this->toPrismTools($request->tools);

        $pendingRequest = Prism::text()
            ->using($provider, $request->model, $providerConfig)
            ->withMessages($messages)
            ->withTools($tools)
            ->withToolChoice($request->requireToolCall ? ToolChoice::Any : ToolChoice::Auto)
            ->withMaxSteps(1);

        return $this->generateTurnFromStream(
            pendingRequest: $this->applyClientConfiguration($pendingRequest),
        );
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

                    $normalized[ 'url' ] = $value;

                    continue;

                }

                if (str_starts_with($value, 'sk-')) {

                    $normalized[ 'api_key' ] = $value;

                    continue;

                }

                if (!array_key_exists('provider', $normalized)) {
                    $normalized[ 'provider' ] = $value;
                }

            }

            $providerConfig = $normalized;

        }

        if (array_key_exists('endpoint', $providerConfig) && !array_key_exists('url', $providerConfig)) {

            $providerConfig[ 'url' ] = $providerConfig[ 'endpoint' ];
            unset($providerConfig[ 'endpoint' ]);

        }

        return $providerConfig;
    }

    private function applyClientConfiguration(PrismPendingTextRequest $pendingRequest): PrismPendingTextRequest
    {
        if (method_exists($pendingRequest, 'withClientOptions')) {

            $clientOptions = $this->driverConfiguration[ 'client_options' ] ?? null;

            if (is_array($clientOptions)) {
                $pendingRequest = $pendingRequest->withClientOptions($clientOptions);
            }

        }

        if (method_exists($pendingRequest, 'withClientRetry')) {

            $retryTimes = $this->driverConfiguration[ 'retry_times' ] ?? null;

            if (is_int($retryTimes)) {

                $retrySleepMilliseconds = $this->driverConfiguration[ 'retry_sleep_milliseconds' ] ?? 0;
                $retrySleepMilliseconds = is_int($retrySleepMilliseconds) ? $retrySleepMilliseconds : 0;
                $pendingRequest = $pendingRequest->withClientRetry($retryTimes, $retrySleepMilliseconds);

            }

        }

        return $pendingRequest;
    }

    private function generateTurnFromStream(PrismPendingTextRequest $pendingRequest): AgentTurnResponse
    {
        $responseText = '';
        $toolCalls = [];
        $toolResults = [];
        $receivedToolCall = false;

        foreach ($pendingRequest->asStream() as $streamEvent) {

            if ($streamEvent instanceof TextDeltaEvent) {

                $responseText .= $streamEvent->delta;

                continue;

            }

            if ($streamEvent instanceof ToolCallEvent) {

                $toolCalls[] = $this->toAgentToolCall($streamEvent->toolCall);
                $receivedToolCall = true;

                continue;

            }

            if ($streamEvent instanceof ToolResultEvent) {

                if ($receivedToolCall) {
                    break;
                }

                $toolResults[] = $this->toAgentToolResult($streamEvent->toolResult);

            }

        }

        return new AgentTurnResponse(
            toolCalls: $toolCalls,
            text: $responseText,
            toolResults: $toolResults,
        );
    }

    /**
     * @param array<int, AgentConversationMessage> $messages
     */
    private function toPrismMessages(array $messages): array
    {
        $prismMessages = [];

        foreach ($messages as $message) {

            if ($message->role === ConversationRole::User) {

                $prismMessages[] = new UserMessage((string) ($message->payload[ 'content' ] ?? ''));

                continue;

            }

            if ($message->role === ConversationRole::Assistant) {

                $prismMessages[] = new AssistantMessage(
                    content: (string) ($message->payload[ 'content' ] ?? ''),
                    toolCalls: $this->toPrismConversationToolCalls($message)->all(),
                );

                continue;

            }

            if ($message->role === ConversationRole::ToolResult) {

                $toolResults = $this->toPrismConversationToolResults($message);

                if ($toolResults->isNotEmpty()) {
                    $prismMessages[] = new ToolResultMessage($toolResults->all());
                }

            }

        }

        return $prismMessages;
    }

    /**
     * @return Collection<ToolCall>
     */
    private function toPrismConversationToolCalls(AgentConversationMessage $message): Collection
    {
        return collect($message->payload[ 'tool_calls' ] ?? [])->map(function (AgentToolCall $toolCall) {

            return new ToolCall(
                id: $toolCall->id,
                name: $toolCall->name,
                arguments: $toolCall->arguments,
                resultId: $toolCall->id,
            );

        });
    }

    /**
     * @return Collection<PrismToolResult>
     */
    private function toPrismConversationToolResults(AgentConversationMessage $message): Collection
    {
        return collect($message->payload[ 'tool_results' ] ?? [])->map(function (AgentToolResult $toolResult) {

            return new PrismToolResult(
                toolCallId: $toolResult->toolCallId,
                toolName: $toolResult->toolName,
                args: $toolResult->arguments,
                result: $toolResult->result->jsonSerialize(),
                toolCallResultId: $toolResult->toolCallId,
            );

        });
    }

    /**
     * @param array<int, AgentToolDefinition> $tools
     * @return array<int, PrismTool>
     */
    private function toPrismTools(array $tools): array
    {
        $prismTools = [];

        foreach ($tools as $tool) {

            $toolSchema = self::schemaToArray($tool->parametersSchema);

            $prismTool = (new PrismTool())
                ->as($tool->name)
                ->for($tool->description)
                ->withProviderOptions([ 'strict' => $tool->strict ])
                ->using(static fn (): string => 'ok');

            $properties = $toolSchema[ 'properties' ] ?? null;

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
     * @throws JsonException
     */
    private static function schemaToArray(Schema $schema): array
    {
        $decodedSchema = json_decode(json_encode($schema, JSON_THROW_ON_ERROR), true, 512, JSON_THROW_ON_ERROR);

        if (!is_array($decodedSchema)) {
            throw new RuntimeException('tool schema must encode into an object payload');
        }

        return $decodedSchema;
    }

    private function toAgentToolCall(ToolCall $toolCall): AgentToolCall
    {
        try {

            $arguments = $toolCall->arguments();

        } catch (Throwable) {

            $arguments = [];

        }

        $toolCallIdentifier = is_string($toolCall->resultId) && $toolCall->resultId !== ''
            ? $toolCall->resultId
            : $toolCall->id;

        return new AgentToolCall($toolCallIdentifier, $toolCall->name, $arguments);
    }

    private function toAgentToolResult(PrismToolResult $toolResult): AgentToolResult
    {
        return new AgentToolResult(
            toolCallId: $toolResult->toolCallId,
            toolName: $toolResult->toolName,
            arguments: $toolResult->args,
            result: $toolResult->result,
        );
    }

    /**
     * @return array<string, mixed>|float|int|string|null
     */
    private function normalizePrismToolResultValue(mixed $value): array|string|int|float|null
    {
        if (is_array($value) || is_string($value) || is_int($value) || is_float($value) || $value === null) {
            return $value;
        }

        if (is_bool($value)) {
            return $value ? 'true' : 'false';
        }

        if (is_object($value) && method_exists($value, 'toArray')) {

            $normalizedValue = $value->toArray();

            if (is_array($normalizedValue)) {
                return $normalizedValue;
            }

        }

        $decodedValue = json_decode(json_encode($value, JSON_UNESCAPED_SLASHES), true);

        if (is_array($decodedValue)) {
            return $decodedValue;
        }

        if (is_string($decodedValue) || is_int($decodedValue) || is_float($decodedValue)) {
            return $decodedValue;
        }

        if (is_bool($decodedValue)) {
            return $decodedValue ? 'true' : 'false';
        }

        return (string) $value;
    }

    private function normalizeConversationToolResultValueForProvider(mixed $value, PrismProvider $provider): array|string|int|float|null
    {
        $normalizedValue = $this->normalizePrismToolResultValue($value);

        if ($provider === PrismProvider::OpenAI) {
            return $normalizedValue;
        }

        if (is_array($normalizedValue)) {
            return json_encode($normalizedValue, JSON_UNESCAPED_SLASHES);
        }

        if ($normalizedValue === null) {
            return '';
        }

        if (is_int($normalizedValue) || is_float($normalizedValue)) {
            return (string) $normalizedValue;
        }

        return $normalizedValue;
    }

    private function resolvePrismProvider(string $providerIdentifier): PrismProvider
    {
        $normalizedIdentifier = strtolower(trim($providerIdentifier));
        $normalizedIdentifier = str_replace([ '-', ' ' ], '', $normalizedIdentifier);
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

        $provider = $providerMap[ $normalizedIdentifier ] ?? null;

        if ($provider !== null) {
            return $provider;
        }

        throw new RuntimeException("unsupported prism provider `{$providerIdentifier}`");
    }
}
