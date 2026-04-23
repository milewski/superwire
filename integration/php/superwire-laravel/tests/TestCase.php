<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Illuminate\Foundation\Application;
use Orchestra\Testbench\TestCase as OrchestraTestCase;
use Prism\Prism\Enums\Provider;
use Prism\Prism\PrismManager;
use Prism\Prism\PrismServiceProvider;
use Prism\Prism\Providers\Provider as PrismProvider;
use Spatie\LaravelData\LaravelDataServiceProvider;
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
            LaravelDataServiceProvider::class,
            SuperwireLaravelServiceProvider::class,
        ];
    }

    protected function getEnvironmentSetUp($app): void
    {
        $configuredPath = env('SUPERWIRE_CLI_PATH');

        if (!is_string($configuredPath) || $configuredPath === '') {
            $configuredPath = $this->resolveDefaultCliPath();
        }

        $app[ 'config' ]->set('superwire.cli.path', $configuredPath);
    }

    protected function resolveDefaultCliPath(): string
    {
        $workspaceRoot = $this->workspaceRoot();

        $candidates = [
            $workspaceRoot . '/target/debug/superwire-cli',
            $workspaceRoot . '/target/release/superwire-cli',
            $workspaceRoot . '/superwire-cli',
            $workspaceRoot . '/crates/cli/target/debug/superwire-cli',
            $workspaceRoot . '/crates/cli/target/release/superwire-cli',
        ];

        foreach ($candidates as $candidate) {

            if ($this->isUsableCliPath($candidate)) {
                return $candidate;
            }

        }

        foreach ($this->globCliPathCandidates($workspaceRoot) as $candidate) {

            if ($this->isUsableCliPath($candidate)) {
                return $candidate;
            }

        }

        return $workspaceRoot . '/target/debug/superwire-cli';
    }

    protected function workspaceRoot(): string
    {
        $workspaceRoot = realpath(__DIR__ . '/../../../..');

        return is_string($workspaceRoot) && $workspaceRoot !== ''
            ? $workspaceRoot
            : dirname(__DIR__, 4);
    }

    protected function isUsableCliPath(string $candidate): bool
    {
        return $candidate !== '' && is_file($candidate);
    }

    /**
     * @return array<int, string>
     */
    protected function globCliPathCandidates(string $workspaceRoot): array
    {
        $patterns = [
            $workspaceRoot . '/target/debug/superwire-cli*',
            $workspaceRoot . '/target/release/superwire-cli*',
            $workspaceRoot . '/target/*/superwire-cli*',
            $workspaceRoot . '/crates/cli/target/debug/superwire-cli*',
            $workspaceRoot . '/crates/cli/target/release/superwire-cli*',
        ];

        $candidates = [];

        foreach ($patterns as $pattern) {

            foreach (glob($pattern) ?: [] as $candidate) {
                $candidates[] = $candidate;
            }

        }

        return array_values(array_unique($candidates));
    }

    protected function compileWorkflow(string $fixtureName): WorkflowDefinition
    {
        return $this->app->make(WorkflowCompiler::class)->compile(__DIR__ . '/stubs/' . $fixtureName);
    }

    protected function shouldUseRealProvider(): bool
    {
        return filter_var(env('SUPERWIRE_TEST_USE_REAL_PROVIDER', false), FILTER_VALIDATE_BOOL);
    }

    /**
     * @return array<string, string>
     */
    protected function realProviderSecrets(): array
    {
        return [
            'model' => (string) env('SUPERWIRE_TEST_MODEL', ''),
            'endpoint' => (string) env('SUPERWIRE_TEST_ENDPOINT', ''),
            'api_key' => (string) env('SUPERWIRE_TEST_API_KEY', ''),
        ];
    }

    /**
     * @param array<string, mixed> $resultsByPrompt
     */
    protected function fakeToolLoopProvider(array $resultsByPrompt): ToolLoopProvider
    {
        $provider = new ToolLoopProvider($resultsByPrompt);

        $this->useFakeProvider($provider);

        return $provider;
    }

    protected function useFakeProvider(PrismProvider $provider): PrismProvider
    {
        app()->instance(PrismManager::class, new class (app(), $provider) extends PrismManager {
            public function __construct($app, private readonly PrismProvider $provider)
            {
                parent::__construct($app);
            }

            public function resolve(Provider|string $name, array $providerConfig = []): PrismProvider
            {
                if (method_exists($this->provider, 'recordProviderConfig')) {
                    $this->provider->recordProviderConfig($providerConfig);
                }

                return $this->provider;
            }
        });

        return $provider;
    }
}
