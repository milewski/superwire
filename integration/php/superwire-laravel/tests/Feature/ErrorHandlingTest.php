<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Feature;

use InvalidArgumentException;
use RuntimeException;
use Superwire\Laravel\Tests\Fakes\FinalizeErrorResponse;
use Superwire\Laravel\Tests\Fakes\NoFinalizationResponse;
use Superwire\Laravel\Tests\TestCase;
use Superwire\Laravel\Workflow;

final class ErrorHandlingTest extends TestCase
{
    public function test_propagates_finalize_error_from_agent(): void
    {
        $this->fakeToolLoopProvider([
            'Write a short welcome message.' => new FinalizeErrorResponse('model could not complete the task'),
        ]);

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('Agent greeting failed: model could not complete the task');

        Workflow::fromFile(__DIR__ . '/../stubs/greeting.wire')->run();
    }

    public function test_fails_when_agent_does_not_finalize_within_step_limit(): void
    {
        config()->set('superwire.runtime.max_agent_tool_steps', 1);

        $this->fakeToolLoopProvider([
            'Write a short welcome message.' => new NoFinalizationResponse('still thinking'),
        ]);

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('Agent greeting did not call finalize_success or finalize_error after 1 tool steps.');

        Workflow::fromFile(__DIR__ . '/../stubs/greeting.wire')->run();
    }

    public function test_validates_required_input_values(): void
    {
        $this->expectException(InvalidArgumentException::class);

        Workflow::fromFile(__DIR__ . '/../stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1 ])
            ->withSecrets([ 'api_key' => 'secret-token', 'model' => 'secret-model' ])
            ->run();
    }

    public function test_validates_required_secret_values(): void
    {
        $this->expectException(InvalidArgumentException::class);

        Workflow::fromFile(__DIR__ . '/../stubs/inputs_secrets_loop.wire')
            ->withInputs([ 'min' => 1, 'max' => 3 ])
            ->withSecrets([ 'api_key' => 'secret-token' ])
            ->run();
    }
}
