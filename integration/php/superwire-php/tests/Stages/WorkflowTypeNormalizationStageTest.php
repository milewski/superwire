<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Stages;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Support\Stages\WorkflowTypeNormalizationStage;

final class WorkflowTypeNormalizationStageTest extends TestCase
{
    public function testItFillsMissingObjectFieldsWithTypeDefaults(): void
    {
        $normalized = (new WorkflowTypeNormalizationStage())->normalize(
            [],
            [
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

        self::assertSame([ 'summary' => '', 'themes' => [] ], $normalized);
    }
}
