<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Laravel\Data\Workflow\WorkflowDefinitionData;
use Superwire\Laravel\Data\WorkflowData;
use Superwire\Laravel\Tools\WorkflowTool;
use Superwire\Laravel\Tools\WorkflowToolBoundInput;
use Superwire\Laravel\Workflow;

final class MainTest extends TestCase
{
    public function test_demo(): void
    {

        $data = WorkflowDefinitionData::fromJson(file_get_contents(__DIR__ . '/minimal.json'));
        $agent = $data->agents->first();

//        Workflow::fromFile()
//            ->withTools([])
//            ->withInputs([ 'question' => 'what changed?' ])
//            ->withSecrets([ 'token' => 'token-value' ])
//            ->run();
    }
}
