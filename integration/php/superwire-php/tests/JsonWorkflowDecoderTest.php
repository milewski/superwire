<?php

declare(strict_types=1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\JsonWorkflowDecoder;

final class JsonWorkflowDecoderTest extends TestCase
{
    public function testItDecodesCompactWorkflowJsonIntoDomainObjects(): void
    {
        $decoder = new JsonWorkflowDecoder();

        $workflowDefinition = $decoder->decodeFromArray([
            'format' => 'superwire_workflow_compact_v1',
            'workflow_path' => 'sample.wire',
            'providers' => [
                [
                    'name' => 'openai',
                    'driver' => 'prism',
                    'models' => [['$ref' => 'secrets.max_model']],
                    'config' => [
                        'endpoint' => 'https://api.openai.com/v1',
                    ],
                ],
            ],
            'agents' => [
                [
                    'name' => 'draft',
                    'provider' => 'openai',
                    'model' => 'gpt-4.1-mini',
                    'prompt' => [
                        '$template' => [
                            'Write update for ',
                            ['$expr' => ['$ref' => 'input.product']],
                        ],
                    ],
                    'output' => [
                        'iteration' => [
                            'workflow_type' => ['kind' => 'string'],
                            'json_schema' => ['type' => 'string'],
                        ],
                        'final_output' => [
                            'workflow_type' => ['kind' => 'string'],
                            'json_schema' => ['type' => 'string'],
                        ],
                    ],
                    'dependencies' => [],
                    'dependents' => [],
                    'batch' => 0,
                ],
            ],
            'output' => [
                'fields' => [
                    'message' => ['$ref' => 'agent.draft'],
                ],
                'contract' => [
                    'workflow_type' => ['kind' => 'object', 'fields' => ['message' => ['kind' => 'string']]],
                    'json_schema' => ['type' => 'object'],
                ],
            ],
            'execution' => [
                'order' => ['draft'],
                'batches' => [['draft']],
                'edges' => [],
            ],
        ]);

        self::assertSame('superwire_workflow_compact_v1', $workflowDefinition->format);
        self::assertSame('sample.wire', $workflowDefinition->workflowPath);
        self::assertCount(1, $workflowDefinition->providers);
        self::assertSame('prism', $workflowDefinition->providers[0]->driver);
        self::assertCount(1, $workflowDefinition->agents);
        self::assertSame('draft', $workflowDefinition->agents[0]->name);
    }

    public function testItRejectsInvalidWorkflowJsonPayload(): void
    {
        $decoder = new JsonWorkflowDecoder();

        $this->expectException(InvalidWorkflowDefinitionException::class);

        $decoder->decodeFromJson('{"invalid": true}');
    }
}
