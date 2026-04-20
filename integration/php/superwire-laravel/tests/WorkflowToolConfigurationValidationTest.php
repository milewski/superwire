<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Superwire\Contracts\Exception\InvalidWorkflowDefinitionException;
use Superwire\Laravel\Tools\WorkflowTool;
use Superwire\Laravel\Tools\WorkflowToolBoundInput;
use Superwire\Laravel\Workflow;

final class WorkflowToolConfigurationValidationTest extends TestCase
{
    protected function setUp(): void
    {
        parent::setUp();

        config()->set('superwire.cli.binary', $this->superwireCliPath());
    }

    public function testItFailsBeforeExecutionWhenWorkflowReferencesUnregisteredTools(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);
        $this->expectExceptionMessage('agent `helper` references unregistered tool `web_search`');

        Workflow::fromFile($this->toolBindingFixturePath())
            ->withTools([])
            ->withInputs([ 'question' => 'what changed?' ])
            ->withSecrets([ 'token' => 'token-value' ])
            ->run();
    }

    public function testItFailsBeforeExecutionWhenToolBindingsDoNotMatchToolSchema(): void
    {
        $this->expectException(InvalidWorkflowDefinitionException::class);
        $this->expectExceptionMessage('agent `helper` tool `secure_lookup` is missing required binding `workspace_id`');

        Workflow::fromFile($this->toolBindingFixturePath())
            ->withTools([
                WebSearchTool::class,
                SecureLookupToolWithMismatchedSchema::class,
                FilterLookupTool::class,
            ])
            ->withInputs([ 'question' => 'what changed?' ])
            ->withSecrets([ 'token' => 'token-value' ])
            ->run();
    }

    private function toolBindingFixturePath(): string
    {
        return __DIR__ . '/../../superwire-php/tests/Stubs/Wire/tool_bindings_resolution.wire';
    }

    private function superwireCliPath(): string
    {
        return __DIR__ . '/../../../../superwire-cli';
    }
}

final class WorkspaceBoundInput extends WorkflowToolBoundInput
{
    public function __construct(
        public int $workspace_id,
    ) {
    }
}

final class FilterBoundInput extends WorkflowToolBoundInput
{
    public function __construct(
        public string $query,
        public int $limit,
    ) {
    }
}

final class WebSearchTool extends WorkflowTool
{
    public static function toolName(): string
    {
        return 'web_search';
    }

    public function invoke(): array
    {
        return [ 'ok' => true ];
    }
}

final class SecureLookupToolWithMismatchedSchema extends WorkflowTool
{
    public static function toolName(): string
    {
        return 'secure_lookup';
    }

    public function invoke(WorkspaceBoundInput $boundInput): array
    {
        return [ 'workspace_id' => $boundInput->workspace_id ];
    }
}

final class FilterLookupTool extends WorkflowTool
{
    public static function toolName(): string
    {
        return 'filter_lookup';
    }

    public function invoke(FilterBoundInput $boundInput): array
    {
        return [
            'query' => $boundInput->query,
            'limit' => $boundInput->limit,
        ];
    }
}
