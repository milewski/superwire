<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Prism\Prism\Facades\Prism;
use Prism\Prism\Testing\TextResponseFake;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\Runtime;

final class MainTest extends TestCase
{
    public function test_demo(): void
    {
//        $fake = Prism::fake([
//            TextResponseFake::make()->withText('[1,2,3,4,5]'),
//            TextResponseFake::make()->withText('"one"'),
//            TextResponseFake::make()->withText('"two"'),
//            TextResponseFake::make()->withText('"three"'),
//            TextResponseFake::make()->withText('"four"'),
//            TextResponseFake::make()->withText('"five"'),
//        ]);

        $definition = WorkflowDefinition::fromJson(file_get_contents(__DIR__ . '/minimum.json'));
        $runtime = new Runtime($definition);
        $result = $runtime->run();

        dd($result);

        $this->assertSame([
            'numbers' => ['one', 'two', 'three', 'four', 'five'],
        ], $result);

        $fake->assertCallCount(6);
        $fake->assertPrompt('generate a sequence of numbers from 1 to 5.');
        $fake->assertPrompt('spell out this number 1.');
        $fake->assertPrompt('spell out this number 5.');
    
//        Workflow::fromFile()
//            ->withTools([])
//            ->withInputs([ 'question' => 'what changed?' ])
//            ->withSecrets([ 'token' => 'token-value' ])
//            ->run();
    }
}
