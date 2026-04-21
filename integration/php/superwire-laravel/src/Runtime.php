<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Support\Collection;
use Prism\Prism\Enums\Provider;
use Prism\Prism\Text\PendingRequest;
use RuntimeException;
use Superwire\Laravel\Data\Workflow\Agent;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;

final class Runtime
{
    public function __construct(
        private WorkflowDefinition $definition,
    )
    {
    }

    public function run()
    {
        /**
         * @var Collection<string, PendingRequest> $requests
         */
        $requests = $this->definition->agents->mapWithKeys(fn (Agent $agent) => [
            $agent->name => prism()
                ->text()
                ->using(Provider::OpenAI, $agent->model, [
                    'url' => 'http://100.118.249.48:3000/v1',
                    'api_key' => 'sk-S2Wcfi5cJhGGhFpTHjHcClDmQoR6IwTx1PNl9cmIZF6Wtuxz',
                ])
                ->withPrompt($agent->prompt),
        ]);

        foreach ($this->definition->execution->order as $agentName) {
            dd($requests[ $agentName ]->asText());
        }

        dd($requests);

    }

    private function intoProvider(string $provider): Provider
    {
        return match ($provider) {
            'openai' => Provider::OpenAI,
            'ollama' => Provider::Ollama,
            default => throw new RuntimeException(sprintf("Unknown provider: {%s}", $provider)),
        };
    }
}
