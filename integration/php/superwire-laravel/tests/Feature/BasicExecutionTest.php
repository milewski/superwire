<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use InvalidArgumentException;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Tools\Internal\FinalizeSuccessTool;
use Superwire\Laravel\Workflow;

final class BasicExecutionTest extends TestCase
{
    public function test_can_run_compiled_greeting_workflow(): void
    {
        $provider = $this->fakeToolLoopProvider([
            'Write a short welcome message.' => 'Welcome aboard!',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/greeting.wire')->run();

        $this->assertSame([ 'greeting' => 'Welcome aboard!' ], $result->output);
        $this->assertSame('Welcome aboard!', $result->agents[ 'greeting' ]->output);
        $this->assertCount(3, $result->agents[ 'greeting' ]->messages);
        $this->assertSame('user', $result->agents[ 'greeting' ]->messages[ 0 ][ 'type' ]);
        $this->assertSame('assistant', $result->agents[ 'greeting' ]->messages[ 1 ][ 'type' ]);
        $this->assertSame('tool_result', $result->agents[ 'greeting' ]->messages[ 2 ][ 'type' ]);
        $this->assertSame(FinalizeSuccessTool::name(), $result->agents[ 'greeting' ]->messages[ 2 ][ 'tool_results' ][ 0 ][ 'tool_name' ]);
        $this->assertCount(0, $provider->textRequests());
        $this->assertCount(1, $provider->streamRequests());
    }

    public function test_can_run_workflow_with_dynamic_inputs_and_secret_provider_config(): void
    {
        $this->fakeToolLoopProvider([
            'generate a sequence of numbers from 1 to 20.' => range(1, 20),
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/dynamic_inputs.wire')
            ->withInputs([ 'min' => 1, 'max' => 20 ])
            ->withSecrets([
                'model' => 'qwen3.5-9b',
                'endpoint' => 'http://localhost/v1',
                'api_key' => 'sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
            ])
            ->run();

        $this->assertSame([ 'numbers' => range(1, 20) ], $result->output);
    }

    public function test_can_disable_streaming_globally(): void
    {
        config()->set('superwire.runtime.stream', false);

        $provider = $this->fakeToolLoopProvider([
            'Write a short welcome message.' => 'Welcome aboard!',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/greeting.wire')->run();

        $this->assertSame([ 'greeting' => 'Welcome aboard!' ], $result->output);
        $this->assertCount(1, $provider->textRequests());
        $this->assertCount(0, $provider->streamRequests());
    }

    public function test_rejects_inputs_for_workflow_without_input_definition(): void
    {
        $this->expectException(InvalidArgumentException::class);
        $this->expectExceptionMessage('workflow does not define input');

        Workflow::fromFile(__DIR__ . '/../stubs/greeting.wire')
            ->withInputs([ 'topic' => 'unexpected' ])
            ->run();
    }
}
