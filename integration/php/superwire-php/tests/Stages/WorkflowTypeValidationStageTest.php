<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Stages;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;

final class WorkflowTypeValidationStageTest extends TestCase
{
    public function testItAcceptsMatchingObjectOutput(): void
    {
        (new WorkflowTypeValidationStage())->validate(
            [ 'summary' => 'ok' ],
            [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
            'agent output',
        );

        self::assertTrue(true);
    }

    public function testItRejectsMismatchedObjectOutput(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);

        (new WorkflowTypeValidationStage())->validate(
            [ 'summary' => 123 ],
            [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
            'agent output',
        );
    }
}
