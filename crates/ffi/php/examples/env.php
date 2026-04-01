<?php

declare(strict_types = 1);

function loadOpenAiProviderSecrets(): array
{
    $openAiEndpoint = (string) (getenv('ENGINE_AI_OPENAI_ENDPOINT') ?: 'http://127.0.0.1:1234/v1');
    $openAiApiKey = (string) (getenv('ENGINE_AI_OPENAI_API_KEY') ?: 'local-api-key');
    $openAiModel = (string) (getenv('ENGINE_AI_OPENAI_MODEL') ?: 'qwen/qwen3.5-35b-a3b');

    return [
        'openai_endpoint' => $openAiEndpoint,
        'openai_api_key' => $openAiApiKey,
        'openai_model' => $openAiModel,
    ];
}
