/**
 * Shared environment loader for all examples.
 *
 * Centralizes required provider environment variables so each example can focus
 * on workflow and runtime behavior.
 */
export type OpenAIProviderSecrets = {
    openai_endpoint: string;
    openai_api_key: string;
    openai_model: string;
}

type OllamaProviderSecrets = {
    ollama_endpoint: string;
    ollama_model: string;
}

function requiredEnvironmentVariable(variableName: string): string {
    const variableValue = process.env[variableName]

    if (!variableValue) {
        throw new Error(`Missing required environment variable: ${ variableName }`)
    }

    return variableValue
}

export function loadOpenAIProviderSecrets(): OpenAIProviderSecrets {
    return {
        openai_endpoint: requiredEnvironmentVariable('ENGINE_AI_OPENAI_ENDPOINT'),
        openai_api_key: requiredEnvironmentVariable('ENGINE_AI_OPENAI_API_KEY'),
        openai_model: requiredEnvironmentVariable('ENGINE_AI_OPENAI_MODEL'),
    }
}

export function loadOllamaProviderSecrets(): OllamaProviderSecrets {
    return {
        ollama_endpoint: requiredEnvironmentVariable('ENGINE_AI_OLLAMA_ENDPOINT'),
        ollama_model: requiredEnvironmentVariable('ENGINE_AI_OLLAMA_MODEL'),
    }
}
