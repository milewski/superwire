<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Stages;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\JsonWorkflowDecoder;
use Superwire\Contracts\Support\Stages\WorkflowDefinitionValidationStage;

final class WorkflowDefinitionValidationStageTest extends TestCase
{
    public function test_it_rejects_unknown_agent_provider(): void
    {
        $definition = (new JsonWorkflowDecoder())->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'test.wire',
            'providers' => [],
            'agents' => [
                [
                    'name' => 'summary',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => 'prompt',
                    'output' => [
                        'iteration' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                        'final_output' => [ 'workflow_type' => [ 'kind' => 'string' ], 'json_schema' => [ 'type' => 'string' ] ],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
            ],
            'output' => [ 'fields' => [], 'contract' => [ 'workflow_type' => [ 'kind' => 'object' ], 'json_schema' => [ 'type' => 'object' ] ] ],
            'execution' => [ 'order' => [ 'summary' ], 'batches' => [ [ 'summary' ] ], 'edges' => [] ],
        ]);

        $this->expectException(InvalidWorkflowDefinitionException::class);

        (new WorkflowDefinitionValidationStage())->validate($definition);
    }
}
