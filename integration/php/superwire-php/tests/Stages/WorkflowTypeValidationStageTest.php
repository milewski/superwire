<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Stages;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;

final class WorkflowTypeValidationStageTest extends TestCase
{
    public function test_it_accepts_matching_object_output(): void
    {
        (new WorkflowTypeValidationStage())->validate(
            value: [ 'summary' => 'ok' ],
            workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
            context: 'agent output',
        );

        $this->assertTrue(true);
    }

    public function test_it_rejects_mismatched_object_output(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);

        (new WorkflowTypeValidationStage())->validate(
            value: [ 'summary' => 123 ],
            workflowType: [ 'kind' => 'object', 'fields' => [ 'summary' => [ 'kind' => 'string' ] ] ],
            context: 'agent output',
        );
    }
}
