<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Closure;
use Illuminate\Support\Facades\Concurrency;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExecutionResult;
use Superwire\Contracts\Contracts\AgentDriverInterface;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;
use Superwire\Contracts\Support\JsonWorkflowDecoder;

final class LaravelWorkflowRunnerTest extends TestCase
{
    public function testItRunsWorkflowBatchesWithDependencies(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $driverRegistry->register('prism', new FakeAgentDriver());

        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);
        $workflowDefinition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'tests.wire',
            'providers' => [
                [
                    'name' => 'openai',
                    'driver' => 'openai',
                    'config' => [],
                ],
            ],
            'agents' => [
                [
                    'name' => 'changelog',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => [
                        '$template' => [
                            'Create changelog for ',
                            [ '$expr' => [ '$ref' => 'input.product' ] ],
                        ],
                    ],
                    'output' => [
                        'iteration' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                        'final_output' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                    ],
                    'dependencies' => [],
                    'dependents' => [ 'review' ],
                    'batch' => 0,
                ],
                [
                    'name' => 'review',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => [
                        '$template' => [
                            'Review output: ',
                            [ '$expr' => [ '$ref' => 'agent.changelog' ] ],
                        ],
                    ],
                    'output' => [
                        'iteration' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                        'final_output' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                    ],
                    'dependencies' => [ 'changelog' ],
                    'dependents' => [],
                    'batch' => 1,
                ],
            ],
            'output' => [
                'fields' => [
                    'review' => [ '$ref' => 'agent.review' ],
                ],
                'contract' => [
                    'workflow_type' => [ 'kind' => 'object', 'fields' => [ 'review' => [ 'kind' => 'string' ] ] ],
                    'json_schema' => [ 'type' => 'object' ],
                ],
            ],
            'execution' => [
                'order' => [ 'changelog', 'review' ],
                'batches' => [ [ 'changelog' ], [ 'review' ] ],
                'edges' => [ [ 'from' => 'changelog', 'to' => 'review' ] ],
            ],
        ]);

        $result = $workflowRunner->run($workflowDefinition, [ 'product' => 'Superwire' ]);

        $this->assertSame('reply: Review output: reply: Create changelog for Superwire', $result->output[ 'review' ]);
    }

    public function testItExecutesForEachAgentsAsArrayOutputs(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $driverRegistry->register('prism', new FakeAgentDriver());

        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);
        $workflowDefinition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'tests.wire',
            'providers' => [
                [
                    'name' => 'openai',
                    'driver' => 'openai',
                    'config' => [],
                ],
            ],
            'agents' => [
                [
                    'name' => 'collector',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => [
                        '$template' => [
                            'item=',
                            [ '$expr' => [ '$ref' => 'item' ] ],
                        ],
                    ],
                    'for_each' => [
                        'pattern' => [ 'identifier' => 'item' ],
                        'iterable' => [ '$ref' => 'input.items' ],
                    ],
                    'output' => [
                        'iteration' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                        'final_output' => [ 'workflow_type' => [ 'kind' => 'array' ], 'json_schema' => [ 'type' => 'array' ] ],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
            ],
            'output' => [
                'fields' => [
                    'items' => [ '$ref' => 'agent.collector' ],
                ],
                'contract' => [
                    'workflow_type' => [ 'kind' => 'object', 'fields' => [ 'items' => [ 'kind' => 'array' ] ] ],
                    'json_schema' => [ 'type' => 'object' ],
                ],
            ],
            'execution' => [
                'order' => [ 'collector' ],
                'batches' => [ [ 'collector' ] ],
                'edges' => [],
            ],
        ]);

        $result = $workflowRunner->run($workflowDefinition, [ 'items' => [ 1, 2, 3 ] ]);

        $this->assertSame([ 'reply: item=1', 'reply: item=2', 'reply: item=3' ], $result->output[ 'items' ]);
    }

    public function testItResolvesModelAndProviderModelsFromSecrets(): void
    {
        config([ 'superwire.parallel.driver' => 'sync' ]);

        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $capturingDriver = new FakeAgentDriver();
        $driverRegistry->register('prism', $capturingDriver);

        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);
        $workflowDefinition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'tests.wire',
            'providers' => [
                [
                    'name' => 'openai',
                    'driver' => 'openai',
                    'models' => [ [ '$ref' => 'secrets.max_model' ] ],
                    'config' => [
                        'models' => [ [ '$ref' => 'secrets.max_model' ] ],
                    ],
                ],
            ],
            'agents' => [
                [
                    'name' => 'summary',
                    'provider' => 'openai',
                    'model' => [
                        '$call' => 'openai',
                        'args' => [ [ '$ref' => 'secrets.max_model' ] ],
                        'named' => [],
                    ],
                    'prompt' => 'run',
                    'output' => [
                        'iteration' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                        'final_output' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
            ],
            'output' => [
                'fields' => [
                    'summary' => [ '$ref' => 'agent.summary' ],
                ],
                'contract' => [
                    'workflow_type' => [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
                    'json_schema' => [ 'type' => 'object' ],
                ],
            ],
            'execution' => [
                'order' => [ 'summary' ],
                'batches' => [ [ 'summary' ] ],
                'edges' => [],
            ],
        ]);

        $result = $workflowRunner->run($workflowDefinition, [], [ 'max_model' => 'gpt-4.1' ]);

        $this->assertSame('reply: run', $result->output[ 'summary' ]);
        $this->assertNotNull($capturingDriver->lastRequest);
        $this->assertSame('gpt-4.1', $capturingDriver->lastRequest->model);
        $this->assertSame([ 'gpt-4.1' ], $capturingDriver->lastRequest->provider->configValue('models'));
        $this->assertSame('string', $capturingDriver->lastRequest->expectedOutput->kind());
    }

    public function testItFailsWhenDriverReturnsUnstructuredOutputForStructuredContract(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $driverRegistry->register('prism', new FakeAgentDriver(fn (): mixed => 'not-json-object'));

        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);
        $workflowDefinition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'tests.wire',
            'providers' => [
                [
                    'name' => 'openai',
                    'driver' => 'openai',
                    'config' => [],
                ],
            ],
            'agents' => [
                [
                    'name' => 'structured',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => 'run',
                    'output' => [
                        'iteration' => [
                            'workflow_type' => [ 'kind' => 'object', 'fields' => [ 'value' => [ 'kind' => 'string' ] ] ],
                            'json_schema' => [ 'type' => 'object' ],
                        ],
                        'final_output' => [
                            'workflow_type' => [ 'kind' => 'object', 'fields' => [ 'value' => [ 'kind' => 'string' ] ] ],
                            'json_schema' => [ 'type' => 'object' ],
                        ],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
            ],
            'output' => [
                'fields' => [
                    'value' => [ '$ref' => 'agent.structured.value' ],
                ],
                'contract' => [
                    'workflow_type' => [ 'kind' => 'object', 'fields' => [ 'value' => [ 'kind' => 'string' ] ] ],
                    'json_schema' => [ 'type' => 'object' ],
                ],
            ],
            'execution' => [
                'order' => [ 'structured' ],
                'batches' => [ [ 'structured' ] ],
                'edges' => [],
            ],
        ]);

        $this->expectException(\Superwire\Contracts\Exception\InvalidWorkflowDefinitionException::class);

        $workflowRunner->run($workflowDefinition);
    }

    public function testItDispatchesIndependentBatchAgentsThroughConcurrencyDriver(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $driverRegistry->register('prism', new FakeAgentDriver());

        $fakeConcurrencyDriver = new FakeConcurrencyDriver();
        Concurrency::swap(new FakeConcurrencyManager($fakeConcurrencyDriver));

        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);
        $workflowDefinition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'tests.wire',
            'providers' => [ [
                'name' => 'openai',
                'driver' => 'openai',
                'config' => [],
            ] ],
            'agents' => [
                [
                    'name' => 'first',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => 'one',
                    'output' => [
                        'iteration' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                        'final_output' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
                [
                    'name' => 'second',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => 'two',
                    'output' => [
                        'iteration' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                        'final_output' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
            ],
            'output' => [
                'fields' => [
                    'first' => [ '$ref' => 'agent.first' ],
                    'second' => [ '$ref' => 'agent.second' ],
                ],
                'contract' => [
                    'workflow_type' => [ 'kind' => 'object', 'fields' => [ 'first' => [ 'kind' => 'string' ], 'second' => [ 'kind' => 'string' ] ] ],
                    'json_schema' => [ 'type' => 'object' ],
                ],
            ],
            'execution' => [
                'order' => [ 'first', 'second' ],
                'batches' => [ [ 'first', 'second' ] ],
                'edges' => [],
            ],
        ]);

        $result = $workflowRunner->run($workflowDefinition);

        $this->assertSame('reply: one', $result->output[ 'first' ]);
        $this->assertSame('reply: two', $result->output[ 'second' ]);
        $this->assertSame([ 2 ], $fakeConcurrencyDriver->taskCounts);
    }
}

final class FakeConcurrencyManager
{
    public function __construct(private readonly FakeConcurrencyDriver $driver)
    {
    }

    public function driver(string|null $name = null): FakeConcurrencyDriver
    {
        return $this->driver;
    }
}

final class FakeConcurrencyDriver
{
    /**
     * @var list<int>
     */
    public array $taskCounts = [];

    /**
     * @param array<string, callable(): mixed> $tasks
     * @return array<string, mixed>
     */
    public function run(array $tasks): array
    {
        $this->taskCounts[] = count($tasks);
        $results = [];

        foreach ($tasks as $taskKey => $task) {
            $results[ $taskKey ] = $task();
        }

        return $results;
    }
}

final class FakeAgentDriver implements AgentDriverInterface
{
    public ?AgentExecutionRequest $lastRequest = null;

    /**
     * @param (Closure(AgentExecutionRequest): mixed)|null $outputFactory
     */
    public function __construct(
        private readonly ?Closure $outputFactory = null,
    ) {
    }

    public function execute(AgentExecutionRequest $request): AgentExecutionResult
    {
        $this->lastRequest = $request;

        $output = $this->outputFactory === null
            ? 'reply: ' . $request->prompt
            : ($this->outputFactory)($request);

        return new AgentExecutionResult(
            output: $output,
            context: [
                'model' => $request->model,
                'agent' => $request->agentName,
            ],
        );
    }
}
