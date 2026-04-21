<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Runtime;
use Superwire\Laravel\Tools\WorkflowTool;
use Superwire\Laravel\Tools\WorkflowToolBoundInput;
use Superwire\Laravel\Workflow;

final class MainTest extends TestCase
{
    public function test_demo(): void
    {

        $definition = WorkflowDefinition::fromJson(file_get_contents(__DIR__ . '/minimal.json'));

        $runtime = new Runtime($definition);

        dd($runtime->run());

//        Workflow::fromFile()
//            ->withTools([])
//            ->withInputs([ 'question' => 'what changed?' ])
//            ->withSecrets([ 'token' => 'token-value' ])
//            ->run();
    }
}
