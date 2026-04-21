<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Concerns;

use Illuminate\Contracts\Container\BindingResolutionException;
use Illuminate\Contracts\Container\CircularDependencyException;
use Prism\Prism\Enums\Provider;
use Prism\Prism\PrismManager;
use Prism\Prism\Providers\Provider as PrismProvider;
use Prism\Prism\Text\PendingRequest;
use RuntimeException;
use Superwire\Laravel\Data\Agent\Agent;

trait ResolvesRuntimeProviders
{
    private function intoProvider(string $provider): Provider
    {
        return match ($provider) {
            'openai' => Provider::OpenAI,
            'ollama' => Provider::Ollama,
            default => throw new RuntimeException(sprintf('Unknown provider: {%s}', $provider)),
        };
    }

    private function agentRequest(Agent $agent): PendingRequest
    {
        $provider = $this->definition->providers->findByName($agent->provider);

        $request = prism()
            ->text()
            ->using(
                $this->intoProvider($provider->driver),
                $this->resolveModel($agent),
                $this->normalizeProviderConfig($provider->config),
            );

        if ($agent->inference->temperature() !== null) {
            $request->usingTemperature($agent->inference->temperature());
        }

        if ($agent->inference->maxTokens() !== null) {
            $request->withMaxTokens($agent->inference->maxTokens());
        }

        if ($agent->inference->topP() !== null) {
            $request->usingTopP($agent->inference->topP());
        }

        return $request;
    }

    private function normalizeProviderConfig(array $providerConfig): array
    {
        $normalizedConfig = $this->resolveConfigReferences($providerConfig);

        if (array_key_exists('endpoint', $normalizedConfig)) {

            $normalizedConfig[ 'url' ] = $normalizedConfig[ 'endpoint' ];
            unset($normalizedConfig[ 'endpoint' ]);

        }

        unset($normalizedConfig[ 'driver' ], $normalizedConfig[ 'models' ]);

        return $normalizedConfig;
    }

    private function resolveConfigReferences(mixed $value): mixed
    {
        if (!is_array($value)) {
            return $value;
        }

        if (array_keys($value) === [ '$ref' ] && is_string($value[ '$ref' ])) {
            return $this->promptParser->resolveReference($value[ '$ref' ], [], [], $this->inputValues, $this->secretValues);
        }

        $resolvedValue = [];

        foreach ($value as $key => $itemValue) {
            $resolvedValue[ $key ] = $this->resolveConfigReferences($itemValue);
        }

        return $resolvedValue;
    }

    private function resolveModel(Agent $agent): string
    {
        if ($agent->model->name !== null) {
            return $agent->model->name;
        }

        if ($agent->model->reference !== null) {

            $resolvedModel = $this->promptParser->resolveReference(
                reference: $agent->model->reference,
                agentOutputs: [],
                inputValues: $this->inputValues,
                secretValues: $this->secretValues,
            );

            if (!is_string($resolvedModel)) {
                throw new RuntimeException(sprintf('Resolved model reference for agent %s must be a string.', $agent->name));
            }

            return $resolvedModel;

        }

        throw new RuntimeException(sprintf('Agent %s does not define a resolvable model.', $agent->name));
    }

    private function shouldStreamResponses(): bool
    {
        return (bool) config('superwire.runtime.stream', true);
    }

    /**
     * @throws CircularDependencyException
     * @throws BindingResolutionException
     */
    private function providerInstance(Agent $agent): PrismProvider
    {
        $provider = $this->definition->providers->findByName($agent->provider);

        return app(PrismManager::class)->resolve(
            $this->intoProvider($provider->driver),
            $this->normalizeProviderConfig($provider->config),
        );
    }
}
