<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use InvalidArgumentException;
use RuntimeException;
use Superwire\Laravel\Workflow;

final class MainTest extends TestCase
{
    public function test_simple_loop(): void
    {
        $result = Workflow::fromFile(__DIR__ . '/stubs/simple_loop.wire')
            ->withSecrets([
                'model' => 'qwen3.5-9b',
                'endpoint' => "http://100.118.249.48:3000/v1",
                'api_key' => "sk-S2Wcfi5cJhGGhFpTHjHcClDmQoR6IwTx1PNl9cmIZF6Wtuxz",
            ])
            ->run();

        $this->assertSame([ 'numbers' => [ 'one', 'two', 'three', 'four', 'five' ] ], $result->output);
    }

    public function test_dynamic_inputs(): void
    {
        $this->fakeToolLoopProvider([
            'generate a sequence of numbers from 1 to 20.' => range(1, 20),
        ]);

        $result = Workflow::fromFile(__DIR__ . '/stubs/dynamic_inputs.wire')
            ->withInputs([ 'min' => 1, 'max' => 20 ])
            ->withSecrets([
                'model' => 'qwen3.5-9b',
                'endpoint' => "http://localhost/v1",
                'api_key' => "sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            ])
            ->run();

        $this->assertSame([ 'numbers' => range(1, 20) ], $result->output);
    }

    public function test_can_run_compiled_greeting_workflow(): void
    {
        $this->fakeToolLoopProvider([
            'Write a short welcome message.' => 'Welcome aboard!',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/stubs/greeting.wire')->run();

        $this->assertSame([ 'greeting' => 'Welcome aboard!' ], $result->output);
        $this->assertSame('Welcome aboard!', $result->agents[ 'greeting' ]->output);
        $this->assertCount(3, $result->agents[ 'greeting' ]->messages);
        $this->assertSame('user', $result->agents[ 'greeting' ]->messages[ 0 ][ 'type' ]);
        $this->assertSame('assistant', $result->agents[ 'greeting' ]->messages[ 1 ][ 'type' ]);
        $this->assertSame('tool_result', $result->agents[ 'greeting' ]->messages[ 2 ][ 'type' ]);
        $this->assertSame('finalize_success', $result->agents[ 'greeting' ]->messages[ 2 ][ 'tool_results' ][ 0 ][ 'tool_name' ]);
    }

    public function test_can_run_inputs_secrets_and_for_each_workflow(): void
    {
        $provider = $this->fakeToolLoopProvider([
            'generate a sequence of numbers from 1 to 3.' => [ 1, 2, 3 ],
            'spell out this number: 1.' => 'one',
            'spell out this number: 2.' => 'two',
            'spell out this number: 3.' => 'three',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, 'max' => 3 ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model' ])
            ->run();

        $this->assertSame([ 'numbers' => [ 'one', 'two', 'three' ] ], $result->output);
        $this->assertSame([ 1, 2, 3 ], $result->agents[ 'numbers' ]->output);
        $this->assertCount(3, $result->agents[ 'counter' ]->iterations);
        $this->assertSame('secret-token', $provider->providerConfigs()[ 0 ][ 'api_key' ]);
        $this->assertSame('http://example.test/v1', $provider->providerConfigs()[ 0 ][ 'url' ]);
    }

    public function test_can_interpolate_input_and_agent_output_values(): void
    {
        $this->fakeToolLoopProvider([
            'Summarize Superwire for developers.' => [
                'summary' => 'Superwire ships workflow execution.',
                'tagline' => 'Automation for teams',
            ],
            'Write a launch note for developers about Superwire ships workflow execution. with tagline Automation for teams.' => [
                'body' => 'Developers can now automate workflows with Superwire.',
            ],
        ]);

        $result = Workflow::fromFile(__DIR__ . '/stubs/interpolation_chain.wire')
            ->withInputs([
                'product_name' => 'Superwire',
                'audience' => 'developers',
            ])
            ->run();

        $this->assertEquals(
            expected: [
                'summary' => 'Superwire ships workflow execution.',
                'body' => 'Developers can now automate workflows with Superwire.',
            ],
            actual: $result->output,
        );
    }

    public function test_applies_inference_settings_to_requests(): void
    {
        $provider = $this->fakeToolLoopProvider([
            'Write a short release readiness note.' => 'Ready for release.',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/stubs/inference.wire')->run();

        $this->assertSame([ 'note' => 'Ready for release.' ], $result->output);
        $this->assertSame(0.2, $provider->requests()[ 0 ]->temperature());
        $this->assertSame(12000, $provider->requests()[ 0 ]->maxTokens());
        $this->assertSame(0.9, $provider->requests()[ 0 ]->topP());
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

        Workflow::fromFile(__DIR__ . '/stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, 'max' => 3 ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model' ])
            ->run();
    }

    public function test_validates_required_input_values(): void
    {
        $this->expectException(InvalidArgumentException::class);

        Workflow::fromFile(__DIR__ . '/stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model', ])
            ->run();
    }
}
