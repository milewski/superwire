<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Workflow;

final class WorkflowFromSourceTest extends TestCase
{
    public function test_can_compile_workflow_from_source(): void
    {
        $workflowSource = file_get_contents(__DIR__ . '/../stubs/greeting.wire');

        $this->assertIsString($workflowSource);

        $definition = Workflow::fromSource($workflowSource, 'greeting.wire')->definition();

        $this->assertSame('superwire_workflow_compact_v1', $definition->format);
    }
}
