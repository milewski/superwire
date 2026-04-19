<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Stages;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Support\Stages\WorkflowTypeNormalizationStage;

final class WorkflowTypeNormalizationStageTest extends TestCase
{
    public function test_it_fills_missing_object_fields_with_type_defaults(): void
    {
        $normalized = (new WorkflowTypeNormalizationStage())->normalize(
            value: [],
            workflowType: [
                'kind' => 'object',
                'fields' => [
                    'summary' => [ 'kind' => 'string' ],
                    'themes' => [
                        'kind' => 'array',
                        'item_type' => [
                            'kind' => 'object',
                            'fields' => [
                                'theme' => [ 'kind' => 'string' ],
                                'times' => [ 'kind' => 'integer' ],
                            ],
                        ],
                    ],
                ],
            ],
        );

        $this->assertSame([ 'summary' => '', 'themes' => [] ], $normalized);
    }
}
