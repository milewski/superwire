<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Prism\Prism\Enums\Provider;
use Prism\Prism\Facades\Prism;
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
        $requests = $this->definition->agents->map(function (Agent $agent) {
            return Prism::text()
                ->using(Provider::OpenAI, $agent->model, [
                    'url' => 'new-base-url',
                ])
                ->withPrompt($agent->prompt);
        });

        dd($requests);

    }
}
