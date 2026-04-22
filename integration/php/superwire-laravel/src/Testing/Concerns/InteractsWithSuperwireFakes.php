<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Testing\Concerns;

use Prism\Prism\Enums\Provider;
use Prism\Prism\PrismManager;
use Prism\Prism\Providers\Provider as PrismProvider;
use Superwire\Laravel\Testing\Fakes\ScriptedToolLoopProvider;
use Superwire\Laravel\Testing\Fakes\ToolLoopProvider;

trait InteractsWithSuperwireFakes
{
    /**
     * @param array<string, mixed> $resultsByPrompt
     */
    protected function fakeToolLoopProvider(array $resultsByPrompt): ToolLoopProvider
    {
        $provider = new ToolLoopProvider($resultsByPrompt);

        $this->useFakeProvider($provider);

        return $provider;
    }

    /**
     * @param array<int, callable> $steps
     */
    protected function fakeScriptedToolLoopProvider(array $steps): ScriptedToolLoopProvider
    {
        $provider = new ScriptedToolLoopProvider($steps);

        $this->useFakeProvider($provider);

        return $provider;
    }

    protected function useFakeProvider(PrismProvider $provider): PrismProvider
    {
        app()->instance(PrismManager::class, new class (app(), $provider) extends PrismManager {
            public function __construct($app, private readonly PrismProvider $provider)
            {
                parent::__construct($app);
            }

            public function resolve(Provider|string $name, array $providerConfig = []): PrismProvider
            {
                if (method_exists($this->provider, 'recordProviderConfig')) {
                    $this->provider->recordProviderConfig($providerConfig);
                }

                return $this->provider;
            }
        });

        return $provider;
    }
}
