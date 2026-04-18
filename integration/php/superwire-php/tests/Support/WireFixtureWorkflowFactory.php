<?php

declare(strict_types = 1);

namespace Superwire\Contracts\Tests\Support;

use RuntimeException;
use Superwire\Contracts\Agent\AgentExecutionRequest;
use Superwire\Contracts\Agent\AgentExpectedOutput;
use Superwire\Contracts\Provider\ProviderExecution;
use Superwire\Contracts\Support\ExpressionResolver;
use Superwire\Contracts\Support\JsonWorkflowDecoder;
use Superwire\Contracts\Tool\ToolExecution;
use Superwire\Contracts\Workflow\WorkflowDefinition;

final class WireFixtureWorkflowFactory
{
    /**
     * @param array<string, mixed> $input
     * @param array<string, mixed> $secrets
     */
    public static function makeAgentExecutionRequest(
        string $fixturePath,
        string $agentName,
        array $input = [],
        array $secrets = [],
    ): AgentExecutionRequest {
        $workflowDefinition = self::compileFixture($fixturePath);
        $agentDefinition = $workflowDefinition->agentByName($agentName);

        if ($agentDefinition === null) {
            throw new RuntimeException("fixture `{$fixturePath}` does not define agent `{$agentName}`");
        }

        $providerDefinition = $workflowDefinition->providerByName($agentDefinition->provider);

        if ($providerDefinition === null) {
            throw new RuntimeException("fixture `{$fixturePath}` defines unknown provider `{$agentDefinition->provider}`");
        }

        $expressionResolver = new ExpressionResolver();
        $runtimeContext = [
            'input' => $input,
            'secrets' => $secrets,
            'agent' => [],
            'context' => [],
        ];

        $resolvedProviderConfig = $expressionResolver->resolve($providerDefinition->config, $runtimeContext);
        $resolvedModelExpression = $expressionResolver->resolve($agentDefinition->model, $runtimeContext);
        $resolvedPrompt = $expressionResolver->resolve($agentDefinition->prompt, $runtimeContext);

        if (!is_array($resolvedProviderConfig)) {
            throw new RuntimeException("fixture `{$fixturePath}` provider config must resolve into an object");
        }

        if (!is_string($resolvedPrompt)) {
            throw new RuntimeException("fixture `{$fixturePath}` prompt must resolve into a string");
        }

        if (!is_array($agentDefinition->output) || !array_key_exists('final_output', $agentDefinition->output)) {
            throw new RuntimeException("fixture `{$fixturePath}` agent `{$agentName}` must define output.final_output");
        }

        $resolvedToolExecutions = [];

        foreach ($agentDefinition->tools as $toolDefinition) {

            $resolvedBindings = [];

            foreach ($toolDefinition[ 'bind' ] as $bindingName => $bindingValue) {

                if (!is_string($bindingName)) {
                    throw new RuntimeException("fixture `{$fixturePath}` tool binding names must be strings");
                }

                $resolvedBindings[ $bindingName ] = $expressionResolver->resolve($bindingValue, $runtimeContext);

            }

            $resolvedToolExecutions[] = new ToolExecution($toolDefinition[ 'name' ], $resolvedBindings);

        }

        return new AgentExecutionRequest(
            agentName: $agentDefinition->name,
            provider: new ProviderExecution($providerDefinition->name, $providerDefinition->driver, $resolvedProviderConfig),
            model: self::resolveModelName($resolvedModelExpression, $providerDefinition->driver),
            prompt: $resolvedPrompt,
            expectedOutput: AgentExpectedOutput::fromContract($agentDefinition->output[ 'final_output' ]),
            tools: $resolvedToolExecutions,
        );
    }

    public static function compileFixture(string $fixturePath): WorkflowDefinition
    {
        self::assertFixtureFileExists($fixturePath);
        $compilerCommand = sprintf(
            '%s workflow to-json %s --compact',
            escapeshellarg(self::compilerBinaryPath()),
            escapeshellarg($fixturePath),
        );
        $compilerOutput = shell_exec($compilerCommand);

        if (!is_string($compilerOutput) || trim($compilerOutput) === '') {
            throw new RuntimeException("failed to compile fixture `{$fixturePath}`");
        }

        return (new JsonWorkflowDecoder())->decodeFromJson($compilerOutput);
    }

    private static function assertFixtureFileExists(string $fixturePath): void
    {
        if (!is_file($fixturePath)) {
            throw new RuntimeException("wire fixture was not found at `{$fixturePath}`");
        }
    }

    private static function compilerBinaryPath(): string
    {
        $configuredBinaryPath = getenv('SUPERWIRE_CLI_BINARY');

        if (is_string($configuredBinaryPath) && $configuredBinaryPath !== '') {
            return $configuredBinaryPath;
        }

        $repositoryRootPath = dirname(__DIR__, 5);

        return $repositoryRootPath . '/superwire-cli';
    }

    private static function resolveModelName(mixed $resolvedModelExpression, string $providerDriver): string
    {
        if (is_array($resolvedModelExpression) && array_key_exists('$call', $resolvedModelExpression)) {

            $callName = $resolvedModelExpression[ '$call' ] ?? null;
            $callArguments = $resolvedModelExpression[ 'args' ] ?? null;

            if (!is_string($callName) || $callName !== $providerDriver) {
                throw new RuntimeException('fixture model call target does not match provider driver');
            }

            if (!is_array($callArguments) || $callArguments === []) {
                throw new RuntimeException('fixture model call must include at least one argument');
            }

            if (!is_string($callArguments[ 0 ])) {
                throw new RuntimeException('fixture model call first argument must resolve into a string');
            }

            return $callArguments[ 0 ];

        }

        if (!is_string($resolvedModelExpression)) {
            throw new RuntimeException('fixture model must resolve into a string');
        }

        return $resolvedModelExpression;
    }
}
