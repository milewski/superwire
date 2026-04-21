<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Runtime;

final class MainTest extends TestCase
{
    public function test_demo(): void
    {
        $definition = WorkflowDefinition::fromJson(file_get_contents(__DIR__ . '/minimum.json'));
        $runtime = new Runtime($definition);
        $result = $runtime->run();

//        Workflow::fromFile()
//            ->withTools([])
//            ->withInputs([ 'question' => 'what changed?' ])
//            ->withSecrets([ 'token' => 'token-value' ])
//            ->run();
    }
}
