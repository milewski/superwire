<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Illuminate\Foundation\Application;
use Orchestra\Testbench\TestCase as OrchestraTestCase;
use Prism\Prism\Enums\Provider;
use Prism\Prism\PrismManager;
use Prism\Prism\PrismServiceProvider;
use Superwire\Laravel\Data\Workflow\WorkflowDefinition;
use Superwire\Laravel\SuperwireLaravelServiceProvider;
use Superwire\Laravel\Tests\Fakes\ToolLoopProvider;
use Superwire\Laravel\WorkflowCompiler;

abstract class TestCase extends OrchestraTestCase
{
    /**
     * @param Application $app
     * @return list<class-string>
     */
    protected function getPackageProviders($app): array
    {
        return [
            PrismServiceProvider::class,
            SuperwireLaravelServiceProvider::class,
        ];
    }

    protected function getEnvironmentSetUp($app): void
    {
        $app['config']->set('superwire.cli.path', realpath(__DIR__ . '/../../../../superwire-cli'));
    }

    protected function compileWorkflow(string $fixtureName): WorkflowDefinition
    {
        return $this->app->make(WorkflowCompiler::class)->compile(__DIR__ . '/stubs/' . $fixtureName);
    }

    /**
     * @param array<string, mixed> $resultsByPrompt
     */
    protected function fakeToolLoopProvider(array $resultsByPrompt): void
    {
        $provider = new ToolLoopProvider($resultsByPrompt);

        app()->instance(PrismManager::class, new class(app(), $provider) extends PrismManager {
            public function __construct($app, private readonly ToolLoopProvider $provider)
            {
                parent::__construct($app);
            }

            public function resolve(Provider|string $name, array $providerConfig = []): \Prism\Prism\Providers\Provider
            {
                return $this->provider;
            }
        });
    }
}
