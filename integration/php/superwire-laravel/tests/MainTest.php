<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Superwire\Laravel\Workflow;

final class MainTest extends TestCase
{
    public function test_can_run_compiled_greeting_workflow(): void
    {
        $this->fakeToolLoopProvider([
            'Write a short welcome message.' => 'Welcome aboard!',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/stubs/greeting.wire')->run();

        $this->assertSame([ 'greeting' => 'Welcome aboard!' ], $result->output);
        $this->assertSame('Welcome aboard!', $result->agents[ 'greeting' ]->output);
    }
}
