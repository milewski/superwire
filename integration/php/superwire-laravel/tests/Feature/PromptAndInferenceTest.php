<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Workflow;

final class PromptAndInferenceTest extends TestCase
{
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

        $result = Workflow::fromFile(__DIR__ . '/../stubs/interpolation_chain.wire')
            ->withInputs([
                'product_name' => 'Superwire',
                'audience' => 'developers',
            ])
            ->run();

        $this->assertSame(
            expected: [
                'body' => 'Developers can now automate workflows with Superwire.',
                'summary' => 'Superwire ships workflow execution.',
            ],
            actual: $result->output,
        );
    }

    public function test_applies_inference_settings_to_requests(): void
    {
        $provider = $this->fakeToolLoopProvider([
            'Write a short release readiness note.' => 'Ready for release.',
        ]);

        $result = Workflow::fromFile(__DIR__ . '/../stubs/inference.wire')->run();

        $this->assertSame([ 'note' => 'Ready for release.' ], $result->output);
        $this->assertSame(0.2, $provider->requests()[ 0 ]->temperature());
        $this->assertSame(12000, $provider->requests()[ 0 ]->maxTokens());
        $this->assertSame(0.9, $provider->requests()[ 0 ]->topP());
    }
}
