<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use Superwire\Contracts\Exception\ExpressionResolutionException;
use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Contracts\Support\ExpressionResolver;
use Superwire\Contracts\Support\Stages\WorkflowTypeValidationStage;
use Superwire\Contracts\Tests\Fakes\WireFixtureWorkflowFactory;

final class WireWorkflowFeatureCoverageTest extends TestCase
{
    public function test_parallel_dependency_agent_prompt_resolves_when_upstream_outputs_are_present(): void
    {
        $reviewRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/parallel_batches.wire',
            agentName: 'review',
            input: [
                'topic' => 'launch',
            ],
            agent: [
                'alpha' => [ 'value' => 'alpha text' ],
                'beta' => [ 'value' => 'beta text' ],
                'gamma' => [ 'value' => 'gamma text' ],
            ],
        );

        $this->assertStringContainsString('alpha text', $reviewRequest->prompt);
        $this->assertStringContainsString('beta text', $reviewRequest->prompt);
        $this->assertStringContainsString('gamma text', $reviewRequest->prompt);
    }

    public function test_parallel_dependency_agent_prompt_fails_when_upstream_output_is_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `agent.gamma.value`');

        WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/parallel_batches.wire',
            agentName: 'review',
            input: [
                'topic' => 'launch',
            ],
            agent: [
                'alpha' => [ 'value' => 'alpha text' ],
                'beta' => [ 'value' => 'beta text' ],
            ],
        );
    }

    public function test_for_each_identifier_iterable_resolves_from_input_values(): void
    {
        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/for_each_input_list.wire',
        );

        $agentDefinition = $workflowDefinition->agentByName('item_note');
        $iterableValues = (new ExpressionResolver())->resolve(
            expression: $agentDefinition?->forEach?->iterable,
            runtimeContext: [
                'input' => [ 'item_ids' => [ 1, 2, 3 ] ],
            ],
        );

        $this->assertSame([ 1, 2, 3 ], $iterableValues);
    }

    public function test_for_each_identifier_iterable_fails_when_input_values_are_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `input.item_ids`');

        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/for_each_input_list.wire',
        );

        $agentDefinition = $workflowDefinition->agentByName('item_note');

        (new ExpressionResolver())->resolve(
            expression: $agentDefinition?->forEach?->iterable,
            runtimeContext: [
                'input' => [],
            ],
        );
    }

    public function test_multiple_provider_requests_use_expected_models_and_resolve_downstream_prompt(): void
    {
        $workflowPath = __DIR__ . '/../Stubs/Wire/multiple_providers_resolution.wire';

        $redactRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: $workflowPath,
            agentName: 'redact',
            input: [
                'notes' => [ 'one', 'two' ],
            ],
        );

        $synthesizeRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: $workflowPath,
            agentName: 'synthesize',
            input: [
                'notes' => [ 'one', 'two' ],
            ],
            agent: [
                'redact' => [
                    'sanitized' => [ 'redacted one', 'redacted two' ],
                ],
            ],
        );

        $this->assertSame('local-small', $redactRequest->model);
        $this->assertSame('cloud-large', $synthesizeRequest->model);
        $this->assertStringContainsString('redacted one', $synthesizeRequest->prompt);
    }

    public function test_multiple_provider_downstream_prompt_fails_when_upstream_agent_output_is_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `agent.redact.sanitized`');

        WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/multiple_providers_resolution.wire',
            agentName: 'synthesize',
            input: [
                'notes' => [ 'one', 'two' ],
            ],
        );
    }

    public function test_schema_field_types_validate_valid_output_payload(): void
    {
        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/schema_field_types.wire',
        );

        $agentDefinition = $workflowDefinition->agentByName('typed_example');
        $workflowType = $agentDefinition?->output[ 'final_output' ][ 'workflow_type' ] ?? [];

        (new WorkflowTypeValidationStage())->validate(
            value: [
                'string_value' => 'text',
                'number_value' => 10,
                'float_value' => 1.2,
                'boolean_value' => true,
                'explicit_null' => null,
                'nullable_string' => null,
                'array_value' => [ 'a', 'b' ],
                'fixed_array' => [ 'x', 'y' ],
                'enum_value' => 'draft',
                'tuple_value' => [ 'x', 2 ],
                'object_value' => [
                    'nested_string' => 'nested',
                    'nested_number' => 4,
                ],
            ],
            workflowType: $workflowType,
            context: 'typed output',
        );

        $this->assertTrue(true);
    }

    public function test_schema_field_types_fail_invalid_output_payload(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);
        $this->expectExceptionMessage('typed output output is missing required field `object_value`');

        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/schema_field_types.wire',
        );

        $agentDefinition = $workflowDefinition->agentByName('typed_example');
        $workflowType = $agentDefinition?->output[ 'final_output' ][ 'workflow_type' ] ?? [];

        (new WorkflowTypeValidationStage())->validate(
            value: [
                'string_value' => 'text',
                'number_value' => 10,
                'float_value' => 1.2,
                'boolean_value' => true,
                'explicit_null' => null,
                'nullable_string' => null,
                'array_value' => [ 'a', 'b' ],
                'fixed_array' => [ 'x', 'y' ],
                'enum_value' => 'draft',
                'tuple_value' => [ 'x', 2 ],
            ],
            workflowType: $workflowType,
            context: 'typed output',
        );
    }

    public function test_deep_contract_validates_nested_output_payload_successfully(): void
    {
        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/structured_output_contract.wire',
        );

        $workflowType = $workflowDefinition->output[ 'contract' ][ 'workflow_type' ] ?? [];

        (new WorkflowTypeValidationStage())->validate(
            value: [
                'version' => 2,
                'report' => [
                    'overview' => [ 'text' => 'ok' ],
                    'metrics' => [ 'confidence' => 1, 'status' => 'ok' ],
                ],
            ],
            workflowType: $workflowType,
            context: 'workflow output',
        );

        $this->assertTrue(true);
    }

    public function test_deep_contract_fails_when_nested_required_field_is_missing(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);
        $this->expectExceptionMessage('workflow output.report.metrics output is missing required field `status`');

        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/structured_output_contract.wire',
        );

        $workflowType = $workflowDefinition->output[ 'contract' ][ 'workflow_type' ] ?? [];

        (new WorkflowTypeValidationStage())->validate(
            value: [
                'version' => 2,
                'report' => [
                    'overview' => [ 'text' => 'ok' ],
                    'metrics' => [ 'confidence' => 1 ],
                ],
            ],
            workflowType: $workflowType,
            context: 'workflow output',
        );
    }
}
