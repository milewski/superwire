<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\ExpressionResolutionException;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;
use Superwire\Contracts\Tests\Fakes\WireFixtureWorkflowFactory;

final class WireWorkflowCompilationAndResolutionTest extends TestCase
{
    public function test_provider_config_resolves_secret_value_when_secret_is_provided(): void
    {
        $agentExecutionRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/secrets_resolution.wire',
            agentName: 'brief_writer',
            input: [
                'topic' => 'roadmap planning',
            ],
            secrets: [
                'api_key' => 'super-secret-key',
            ],
        );

        $this->assertSame('super-secret-key', $agentExecutionRequest->provider->configValue('api_key'));
    }

    public function test_provider_config_fails_when_required_secret_is_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `secrets.api_key`');

        WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/secrets_resolution.wire',
            agentName: 'brief_writer',
            input: [
                'topic' => 'roadmap planning',
            ],
        );
    }

    public function test_tool_bindings_resolve_user_and_secret_values(): void
    {
        $agentExecutionRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/tool_bindings_resolution.wire',
            agentName: 'helper',
            input: [
                'question' => 'what changed?',
            ],
            secrets: [
                'token' => 'bound-token',
            ],
        );

        $this->assertSame([], $agentExecutionRequest->tools[ 0 ]->bindings);
        $this->assertSame([ 'token' => 'bound-token' ], $agentExecutionRequest->tools[ 1 ]->bindings);
        $this->assertSame([ 'limit' => 5, 'query' => 'what changed?' ], $agentExecutionRequest->tools[ 2 ]->bindings);
    }

    public function test_tool_bindings_fail_when_required_input_is_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `input.question`');

        WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/tool_bindings_resolution.wire',
            agentName: 'helper',
            secrets: [
                'token' => 'bound-token',
            ],
        );
    }

    public function test_structured_output_contract_validates_expected_payload_shape(): void
    {
        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/structured_output_contract.wire',
        );
        $workflowType = $workflowDefinition->output[ 'contract' ][ 'workflow_type' ] ?? [];

        (new WorkflowTypeValidationStage())->validate(
            [
                'version' => 2,
                'report' => [
                    'overview' => [ 'text' => 'ok' ],
                    'metrics' => [
                        'confidence' => 1,
                        'status' => 'ok',
                    ],
                ],
            ],
            $workflowType,
            'workflow output',
        );

        $this->assertTrue(true);
    }

    public function test_structured_output_contract_fails_when_payload_shape_is_invalid(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);
        $this->expectExceptionMessage('workflow output.report.overview output is missing required field `text`');

        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/structured_output_contract.wire',
        );
        $workflowType = $workflowDefinition->output[ 'contract' ][ 'workflow_type' ] ?? [];

        (new WorkflowTypeValidationStage())->validate(
            [
                'version' => 2,
                'report' => [
                    'overview' => [],
                    'metrics' => [
                        'confidence' => 1,
                        'status' => 'ok',
                    ],
                ],
            ],
            $workflowType,
            'workflow output',
        );
    }
}
