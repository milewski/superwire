<?php

declare(strict_types=1);

namespace Superwire\Laravel\Tests;

use Superwire\Contracts\AgentExecutionRequest;
use Superwire\Contracts\AgentExecutionResult;
use Superwire\Contracts\Contracts\AgentDriverInterface;
use Superwire\Contracts\Contracts\DriverRegistryInterface;
use Superwire\Contracts\Contracts\WorkflowRunnerInterface;
use Superwire\Contracts\Support\JsonWorkflowDecoder;

final class LaravelWorkflowRunnerTest extends TestCase
{
    public function testItRunsWorkflowBatchesWithDependencies(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $driverRegistry->register('fake', new FakeAgentDriver());

        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);
        $workflowDefinition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'tests.wire',
            'providers' => [
                [
                    'name' => 'openai',
                    'driver' => 'fake',
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
                            ['$expr' => ['$ref' => 'input.product']],
                        ],
                    ],
                    'output' => [
                        'iteration' => ['workflow_type' => ['kind' => 'string'], 'json_schema' => ['type' => 'string']],
                        'final_output' => ['workflow_type' => ['kind' => 'string'], 'json_schema' => ['type' => 'string']],
                    ],
                    'dependencies' => [],
                    'dependents' => ['review'],
                    'batch' => 0,
                ],
                [
                    'name' => 'review',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => [
                        '$template' => [
                            'Review output: ',
                            ['$expr' => ['$ref' => 'agent.changelog']],
                        ],
                    ],
                    'output' => [
                        'iteration' => ['workflow_type' => ['kind' => 'string'], 'json_schema' => ['type' => 'string']],
                        'final_output' => ['workflow_type' => ['kind' => 'string'], 'json_schema' => ['type' => 'string']],
                    ],
                    'dependencies' => ['changelog'],
                    'dependents' => [],
                    'batch' => 1,
                ],
            ],
            'output' => [
                'fields' => [
                    'review' => ['$ref' => 'agent.review'],
                ],
                'contract' => [
                    'workflow_type' => ['kind' => 'object', 'fields' => ['review' => ['kind' => 'string']]],
                    'json_schema' => ['type' => 'object'],
                ],
            ],
            'execution' => [
                'order' => ['changelog', 'review'],
                'batches' => [['changelog'], ['review']],
                'edges' => [['from' => 'changelog', 'to' => 'review']],
            ],
        ]);

        $result = $workflowRunner->run($workflowDefinition, ['product' => 'Superwire']);

        self::assertSame('reply: Review output: reply: Create changelog for Superwire', $result->output['review']);
    }

    public function testItExecutesForEachAgentsAsArrayOutputs(): void
    {
        $driverRegistry = $this->app->make(DriverRegistryInterface::class);
        $driverRegistry->register('fake', new FakeAgentDriver());

        $workflowRunner = $this->app->make(WorkflowRunnerInterface::class);
        $workflowDefinition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'tests.wire',
            'providers' => [
                [
                    'name' => 'openai',
                    'driver' => 'fake',
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
                            ['$expr' => ['$ref' => 'item']],
                        ],
                    ],
                    'for_each' => [
                        'pattern' => ['identifier' => 'item'],
                        'iterable' => ['$ref' => 'input.items'],
                    ],
                    'output' => [
                        'iteration' => ['workflow_type' => ['kind' => 'string'], 'json_schema' => ['type' => 'string']],
                        'final_output' => ['workflow_type' => ['kind' => 'array'], 'json_schema' => ['type' => 'array']],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
            ],
            'output' => [
                'fields' => [
                    'items' => ['$ref' => 'agent.collector'],
                ],
                'contract' => [
                    'workflow_type' => ['kind' => 'object', 'fields' => ['items' => ['kind' => 'array']]],
                    'json_schema' => ['type' => 'object'],
                ],
            ],
            'execution' => [
                'order' => ['collector'],
                'batches' => [['collector']],
                'edges' => [],
            ],
        ]);

        $result = $workflowRunner->run($workflowDefinition, ['items' => [1, 2, 3]]);

        self::assertSame(['reply: item=1', 'reply: item=2', 'reply: item=3'], $result->output['items']);
    }
}

final class FakeAgentDriver implements AgentDriverInterface
{
    public function execute(AgentExecutionRequest $request): AgentExecutionResult
    {
        return new AgentExecutionResult(
            output: 'reply: ' . $request->prompt,
            context: [
                'model' => $request->model,
                'agent' => $request->agentName,
            ]
        );
    }
}
