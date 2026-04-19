<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests;

use PHPUnit\Framework\TestCase;
use RuntimeException;
use Superwire\Contracts\Exception\ExpressionResolutionException;
use Superwire\Contracts\Support\ExpressionResolver;
use Superwire\Contracts\Tests\Fakes\WireFixtureWorkflowFactory;

final class WireWorkflowPromptAndDependencyValidationTest extends TestCase
{
    public function test_template_function_prompt_resolves_into_final_prompt_text(): void
    {
        $agentExecutionRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/template_function_prompt.wire',
            agentName: 'research_brief',
            input: [
                'study_name' => 'Retention Study',
                'audience' => 'Product Team',
                'findings' => [ 'Users want shortcuts' ],
            ],
        );

        $this->assertStringContainsString('Study: Retention Study', $agentExecutionRequest->prompt);
        $this->assertStringContainsString('Audience: Product Team', $agentExecutionRequest->prompt);
        $this->assertStringContainsString('Users want shortcuts', $agentExecutionRequest->prompt);
    }

    public function test_template_function_fails_when_binding_reference_is_missing(): void
    {
        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('unknown_input_field_reference');

        WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/template_function_invalid_reference.wire',
        );
    }

    public function test_multiline_string_interpolation_resolves_with_upstream_agent_output(): void
    {
        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/multiline_prompt_interpolation.wire',
        );

        $finalMessageAgentDefinition = $workflowDefinition->agentByName('final_message');

        $agentExecutionRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/multiline_prompt_interpolation.wire',
            agentName: 'final_message',
            input: [
                'product_name' => 'Atlas',
            ],
            agent: [
                'draft' => [
                    'text' => 'Initial draft body',
                ],
            ],
        );

        $this->assertSame([ 'draft' ], $finalMessageAgentDefinition?->dependencies);
        $this->assertStringContainsString('Atlas', $agentExecutionRequest->prompt);
        $this->assertStringContainsString('Initial draft body', $agentExecutionRequest->prompt);
    }

    public function test_multiline_string_interpolation_fails_when_upstream_agent_output_is_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `agent.draft.text`');

        WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/multiline_prompt_interpolation.wire',
            agentName: 'final_message',
            input: [
                'product_name' => 'Atlas',
            ],
        );
    }

    public function test_inference_expression_resolves_from_input_values(): void
    {
        $agentExecutionRequest = WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/inference_from_input_reference.wire',
            agentName: 'summary',
            input: [
                'max_tokens' => 2048,
                'topic' => 'planning',
            ],
        );

        $this->assertSame(2048, $agentExecutionRequest->inference[ 'max_tokens' ] ?? null);
        $this->assertSame(0.2, $agentExecutionRequest->inference[ 'temperature' ] ?? null);
    }

    public function test_inference_expression_fails_when_required_input_value_is_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `input.max_tokens`');

        $workflowDefinition = WireFixtureWorkflowFactory::compileFixture(
            __DIR__ . '/../Stubs/Wire/inference_from_input_reference.wire',
        );

        $summaryAgentDefinition = $workflowDefinition->agentByName('summary');

        (new ExpressionResolver())->resolve(
            expression: $summaryAgentDefinition?->inference,
            runtimeContext: [
                'input' => [],
            ],
        );
    }

    public function test_bounded_tool_input_from_upstream_agent_fails_when_agent_context_is_missing(): void
    {
        $this->expectException(ExpressionResolutionException::class);
        $this->expectExceptionMessage('unable to resolve `agent.source.token`');

        WireFixtureWorkflowFactory::makeAgentExecutionRequest(
            fixturePath: __DIR__ . '/../Stubs/Wire/agent_output_bounded_tool_input.wire',
            agentName: 'consumer',
            input: [
                'topic' => 'planning',
            ],
        );
    }
}
