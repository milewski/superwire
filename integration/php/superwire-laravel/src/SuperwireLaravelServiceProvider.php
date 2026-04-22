<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Support\ServiceProvider;

final class SuperwireLaravelServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->mergeConfigFrom(__DIR__ . '/../config/superwire.php', 'superwire');

        $this->app->singleton(WorkflowExecutor::class, function (): WorkflowExecutor {

            $configuredCliPath = (string) config('superwire.cli.path', '');

            if ($configuredCliPath === '') {
                $configuredCliPath = (string) config('superwire.cli.binary', '');
            }

            return new WorkflowExecutor($configuredCliPath);

        });

        $this->app->singleton(WorkflowCompiler::class, function (): WorkflowCompiler {
            return new WorkflowCompiler($this->app->make(WorkflowExecutor::class));
        });

        config()->set(
            'prism.providers',
            array_replace_recursive(
                config('prism.providers', []),
                config('superwire.prism.providers', []),
            ),
        );
    }

    public function boot(): void
    {
        $this->publishes([ __DIR__ . '/../config/superwire.php' => config_path('superwire.php') ], 'superwire-config');
    }
}
