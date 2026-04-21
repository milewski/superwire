<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use RuntimeException;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Workflow;

final class LoopExecutionTest extends TestCase
{
    public function test_can_run_inputs_secrets_and_for_each_workflow(): void
    {
        $provider = $this->fakeToolLoopProvider([
            'generate a sequence of numbers from 1 to 3.' => [ 1, 2, 3 ],
            'spell out this number: 1.' => 'one',
            'spell out this number: 2.' => 'two',
            'spell out this number: 3.' => 'three',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, 'max' => 3 ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model' ])
            ->run();

        $this->assertSame([ 'numbers' => [ 'one', 'two', 'three' ] ], $result->output);
        $this->assertSame([ 1, 2, 3 ], $result->agents[ 'numbers' ]->output);
        $this->assertCount(3, $result->agents[ 'counter' ]->iterations);
        $this->assertSame('secret-token', $provider->providerConfigs()[ 0 ][ 'api_key' ]);
        $this->assertSame('http://example.test/v1', $provider->providerConfigs()[ 0 ][ 'url' ]);
    }

    public function test_executes_parallel_batch_workflow(): void
    {
        $this->fakeToolLoopProvider([
            'Write customer story.' => 'customer story',
            'Write investor story.' => 'investor story',
            'Combine customer story and investor story.' => 'combined review',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/parallel_batch.wire')->run();

        $this->assertSame([ 'review' => 'combined review' ], $result->output);
        $this->assertSame('customer story', $result->agents[ 'customer_story' ]->output);
        $this->assertSame('investor story', $result->agents[ 'investor_story' ]->output);
        $this->assertSame('combined review', $result->agents[ 'review' ]->output);
    }

    public function test_bubbles_real_exception_for_forked_iterations(): void
    {
        $this->fakeToolLoopProvider([
            'generate a sequence of numbers from 1 to 3.' => [ 1, 2, 3 ],
            'spell out this number: 1.' => 'one',
            'spell out this number: 2.' => new RuntimeException('OpenAI: unhandled finish reason "unknown" (status: n/a, type: n/a)'),
            'spell out this number: 3.' => 'three',
        ]);

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('Execution failed for iteration agent counter: RuntimeException: OpenAI: unhandled finish reason "unknown" (status: n/a, type: n/a)');

        Workflow::fromFile(__DIR__ . '/../stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, 'max' => 3 ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model' ])
            ->run();
    }
}
